use axum::{
    extract::{Path, Query, State},
    http::{header, HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use hub_core::{
    auth::{can_publish, can_yank, AuthContext},
    model::{
        CompatibilityIndex, CompatibilitySkillHit, NamespaceRole, PublishSkillRequest, Visibility,
    },
};
use hub_search::{lexical_rank, SearchDocument};
use serde::Deserialize;
use serde_json::json;
use sha2::{Digest, Sha256};
use std::{io::Write, path::PathBuf};

use crate::{error::ApiError, state::AppState};

#[derive(Debug, Deserialize)]
pub struct PublishQuery {
    visibility: Option<Visibility>,
}

#[derive(Debug, Deserialize)]
pub struct SearchQuery {
    q: Option<String>,
    query: Option<String>,
    limit: Option<usize>,
}

#[derive(Debug, Deserialize)]
pub struct YankRequest {
    reason: Option<String>,
}

pub async fn healthz() -> Json<serde_json::Value> {
    Json(json!({"status": "ok"}))
}

pub async fn readyz(State(state): State<AppState>) -> Json<serde_json::Value> {
    let ready = state.repository.is_some();
    Json(json!({"status": if ready { "ready" } else { "fixture" }}))
}

pub async fn well_known_agent_skills() -> Json<serde_json::Value> {
    Json(json!({
        "schema_version": "0.1",
        "registry": {
            "type": "agentenv-skills-hub",
            "index": "/index.json",
            "api": "/api/v1"
        }
    }))
}

pub async fn index_json(
    State(state): State<AppState>,
) -> Result<Json<CompatibilityIndex>, ApiError> {
    Ok(Json(index_for_state(&state).await?))
}

pub async fn list_skills(
    State(state): State<AppState>,
) -> Result<Json<CompatibilityIndex>, ApiError> {
    Ok(Json(index_for_state(&state).await?))
}

pub async fn get_skill(
    State(state): State<AppState>,
    Path((_namespace, name)): Path<(String, String)>,
) -> Result<Json<CompatibilityIndex>, ApiError> {
    let index = filter_index(index_for_state(&state).await?, |hit| hit.name == name);
    if index.skills.is_empty() {
        return Err(ApiError {
            status: StatusCode::NOT_FOUND,
            message: format!("skill `{name}` was not found"),
        });
    }
    Ok(Json(index))
}

pub async fn list_versions(
    State(state): State<AppState>,
    Path((_namespace, name)): Path<(String, String)>,
) -> Result<Json<CompatibilityIndex>, ApiError> {
    get_skill(State(state), Path((_namespace, name))).await
}

pub async fn get_version(
    State(state): State<AppState>,
    Path((_namespace, name, version)): Path<(String, String, String)>,
) -> Result<Json<CompatibilitySkillHit>, ApiError> {
    index_for_state(&state)
        .await?
        .skills
        .into_iter()
        .find(|hit| hit.name == name && hit.version == version)
        .map(Json)
        .ok_or_else(|| ApiError {
            status: StatusCode::NOT_FOUND,
            message: format!("skill `{name}` version `{version}` was not found"),
        })
}

pub async fn publish_version(
    State(state): State<AppState>,
    Path((_namespace, name)): Path<(String, String)>,
    Query(query): Query<PublishQuery>,
    headers: HeaderMap,
    Json(request): Json<PublishSkillRequest>,
) -> Result<(StatusCode, Json<hub_core::model::SkillVersionRecord>), ApiError> {
    if request.manifest.name != name {
        return Err(ApiError::bad_request(format!(
            "path skill name `{name}` does not match manifest name `{}`",
            request.manifest.name
        )));
    }
    let auth = auth_from_headers(&headers)?;
    can_publish(&auth, &_namespace)?;
    let service = state
        .service
        .as_ref()
        .ok_or_else(|| ApiError::unavailable("hub repository is not configured"))?;
    let record = service
        .publish(
            &auth,
            &_namespace,
            query.visibility.unwrap_or(Visibility::Public),
            &request,
        )
        .await?;
    Ok((StatusCode::CREATED, Json(record)))
}

pub async fn yank_version(
    State(state): State<AppState>,
    Path((namespace, name, version)): Path<(String, String, String)>,
    headers: HeaderMap,
    Json(request): Json<YankRequest>,
) -> Result<StatusCode, ApiError> {
    let auth = auth_from_headers(&headers)?;
    let service = state
        .service
        .as_ref()
        .ok_or_else(|| ApiError::unavailable("hub repository is not configured"))?;
    service
        .yank(
            &auth,
            &namespace,
            &name,
            &version,
            request.reason.as_deref().unwrap_or("yanked by API"),
        )
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn unyank_version(
    State(state): State<AppState>,
    Path((namespace, name, version)): Path<(String, String, String)>,
    headers: HeaderMap,
) -> Result<StatusCode, ApiError> {
    let auth = auth_from_headers(&headers)?;
    can_yank(&auth, &namespace)?;
    let repository = state
        .repository
        .as_ref()
        .ok_or_else(|| ApiError::unavailable("hub repository is not configured"))?;
    repository
        .unyank_version(&namespace, &name, &version)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn search(
    State(state): State<AppState>,
    Query(query): Query<SearchQuery>,
) -> Result<Json<CompatibilityIndex>, ApiError> {
    let mut index = index_for_state(&state).await?;
    let limit = query.limit;
    let Some(search_terms) = query.q.or(query.query) else {
        truncate_index(&mut index, limit);
        return Ok(Json(index));
    };
    let docs = index
        .skills
        .iter()
        .map(|hit| SearchDocument {
            namespace: hit.registry.clone(),
            name: hit.name.clone(),
            version: hit.version.clone(),
            description: hit.description.clone(),
        })
        .collect();
    let ranked = lexical_rank(&search_terms, docs);
    let mut skills = ranked
        .into_iter()
        .filter_map(|doc| {
            index
                .skills
                .iter()
                .find(|hit| hit.name == doc.name && hit.version == doc.version)
                .cloned()
        })
        .collect::<Vec<_>>();
    if let Some(limit) = limit {
        skills.truncate(limit);
    }
    Ok(Json(CompatibilityIndex { skills }))
}

pub async fn similar_search() -> (StatusCode, Json<CompatibilityIndex>) {
    (StatusCode::OK, Json(CompatibilityIndex { skills: vec![] }))
}

pub async fn list_webhooks() -> Json<serde_json::Value> {
    Json(json!({"subscriptions": []}))
}

pub async fn create_webhook() -> (StatusCode, Json<serde_json::Value>) {
    (StatusCode::ACCEPTED, Json(json!({"status": "accepted"})))
}

pub async fn delete_webhook(Path(_id): Path<String>) -> StatusCode {
    StatusCode::NO_CONTENT
}

pub async fn fixture_artifact(
    Path((_name, artifact)): Path<(String, String)>,
) -> Result<Response, ApiError> {
    if artifact.ends_with(".tar.zst") {
        let bytes = fixture_tarball_bytes()?;
        return Ok(([(header::CONTENT_TYPE, "application/octet-stream")], bytes).into_response());
    }
    if artifact.ends_with(".tar.zst.sig") {
        return Ok(fixture_signature().into_response());
    }

    Ok(StatusCode::NOT_FOUND.into_response())
}

pub async fn metrics() -> &'static str {
    "# HELP agentenv_skills_hub_info Skill hub info\n# TYPE agentenv_skills_hub_info gauge\nagentenv_skills_hub_info 1\n"
}

async fn index_for_state(state: &AppState) -> Result<CompatibilityIndex, ApiError> {
    if let Some(repository) = &state.repository {
        return Ok(repository.compatibility_index(&state.registry).await?);
    }
    fixture_index(&state.registry)
}

fn fixture_index(registry: &str) -> Result<CompatibilityIndex, ApiError> {
    Ok(CompatibilityIndex {
        skills: vec![CompatibilitySkillHit {
            name: "code-review".to_owned(),
            version: "1.2.0".to_owned(),
            description: Some("Review code changes".to_owned()),
            registry: registry.to_owned(),
            digest: Some(sha256_digest(&fixture_tarball_bytes()?)),
            signature_ed25519: Some(fixture_signature().trim().to_owned()),
            public_key_ed25519: Some("bb".repeat(32)),
        }],
    })
}

fn filter_index(
    index: CompatibilityIndex,
    predicate: impl Fn(&CompatibilitySkillHit) -> bool,
) -> CompatibilityIndex {
    CompatibilityIndex {
        skills: index.skills.into_iter().filter(predicate).collect(),
    }
}

fn truncate_index(index: &mut CompatibilityIndex, limit: Option<usize>) {
    if let Some(limit) = limit {
        index.skills.truncate(limit);
    }
}

fn fixture_tarball_bytes() -> Result<Vec<u8>, ApiError> {
    let mut tar_bytes = Vec::new();
    {
        let encoder = zstd::stream::write::Encoder::new(&mut tar_bytes, 0).map_err(|source| {
            ApiError::internal(format!("failed to create zstd encoder: {source}"))
        })?;
        let mut builder = tar::Builder::new(encoder);
        append_bytes(
            &mut builder,
            "skill.yaml",
            include_bytes!("../../../tests/fixtures/code-review-skill/skill.yaml"),
        )?;
        append_bytes(
            &mut builder,
            "SKILL.md",
            include_bytes!("../../../tests/fixtures/code-review-skill/SKILL.md"),
        )?;
        let encoder = builder
            .into_inner()
            .map_err(|source| ApiError::internal(format!("failed to finish tar: {source}")))?;
        encoder
            .finish()
            .map_err(|source| ApiError::internal(format!("failed to finish zstd: {source}")))?;
    }

    Ok(tar_bytes)
}

fn fixture_signature() -> &'static str {
    "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\n"
}

fn append_bytes<W: Write>(
    builder: &mut tar::Builder<W>,
    path: &str,
    bytes: &[u8],
) -> Result<(), ApiError> {
    let mut header = tar::Header::new_gnu();
    header.set_size(bytes.len() as u64);
    header.set_mode(0o644);
    header.set_cksum();
    builder
        .append_data(&mut header, PathBuf::from(path), bytes)
        .map_err(|source| ApiError::internal(format!("failed to append fixture file: {source}")))
}

fn auth_from_headers(headers: &HeaderMap) -> Result<AuthContext, ApiError> {
    let Some(subject) = header_value(headers, "x-agentenv-subject")? else {
        return Ok(AuthContext::anonymous());
    };
    let scopes = header_value(headers, "x-agentenv-scopes")?
        .unwrap_or_default()
        .split(',')
        .map(str::trim)
        .filter(|scope| !scope.is_empty())
        .map(str::to_owned)
        .collect::<Vec<_>>();
    let roles = header_value(headers, "x-agentenv-roles")?
        .unwrap_or_default()
        .split(',')
        .map(str::trim)
        .filter(|role| !role.is_empty())
        .map(parse_role)
        .collect::<Result<Vec<_>, _>>()?;

    Ok(AuthContext::new(subject, scopes, roles))
}

fn header_value(headers: &HeaderMap, name: &'static str) -> Result<Option<String>, ApiError> {
    headers
        .get(name)
        .map(|value| {
            value
                .to_str()
                .map(str::to_owned)
                .map_err(|_| ApiError::bad_request(format!("header `{name}` must be valid UTF-8")))
        })
        .transpose()
}

fn parse_role(value: &str) -> Result<(String, NamespaceRole), ApiError> {
    let (namespace, role) = value
        .split_once(':')
        .or_else(|| value.split_once('='))
        .ok_or_else(|| ApiError::bad_request("role entries must use namespace:role"))?;
    let role = match role {
        "reader" => NamespaceRole::Reader,
        "publisher" => NamespaceRole::Publisher,
        "admin" => NamespaceRole::Admin,
        _ => {
            return Err(ApiError::bad_request(format!(
                "unknown namespace role `{role}`"
            )))
        }
    };
    Ok((namespace.to_owned(), role))
}

fn sha256_digest(bytes: &[u8]) -> String {
    format!("sha256:{:x}", Sha256::digest(bytes))
}
