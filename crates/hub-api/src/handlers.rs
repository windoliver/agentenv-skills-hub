use std::{
    collections::{BTreeMap, BTreeSet},
    io::{Cursor, Read},
    path::Path as FsPath,
};

use axum::{
    extract::{Path, Query, State},
    http::{header, HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use hub_core::{
    auth::{can_publish, can_yank, AuthContext},
    model::{
        ArtifactPointer, CompatibilityIndex, CompatibilitySkillHit, NamespaceRole,
        PublishSkillRequest, SkillManifest, Visibility,
    },
    validation::validate_skill_path,
};
use hub_search::SemanticSearch;
use serde::Deserialize;
use serde_json::json;
use sha2::{Digest, Sha256};

use crate::{
    error::ApiError,
    read_model::{
        filter_index, filtered_search_index, fixture_signature, fixture_tarball_bytes,
        index_for_state, index_for_state_and_namespace, SearchParams,
    },
    state::AppState,
};

#[derive(Debug, Deserialize)]
pub struct PublishQuery {
    visibility: Option<Visibility>,
}

#[derive(Debug, Deserialize)]
pub struct SearchQuery {
    q: Option<String>,
    query: Option<String>,
    namespace: Option<String>,
    limit: Option<usize>,
}

#[derive(Debug, Deserialize)]
pub struct SimilarSearchRequest {
    embedding: Vec<f32>,
    limit: Option<i64>,
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
            "api": "/api/v1",
            "mcp": "/mcp"
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
    Query(query): Query<SearchQuery>,
) -> Result<Json<CompatibilityIndex>, ApiError> {
    filtered_search_index(
        &state,
        SearchParams {
            query: query.q.or(query.query),
            namespace: query.namespace,
            limit: query.limit,
        },
    )
    .await
    .map(Json)
}

pub async fn get_skill(
    State(state): State<AppState>,
    Path((namespace, name)): Path<(String, String)>,
) -> Result<Json<CompatibilityIndex>, ApiError> {
    let index = filter_index(
        index_for_state_and_namespace(&state, Some(&namespace)).await?,
        |hit| hit.name == name,
    );
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
    Path((namespace, name)): Path<(String, String)>,
) -> Result<Json<CompatibilityIndex>, ApiError> {
    get_skill(State(state), Path((namespace, name))).await
}

pub async fn get_version(
    State(state): State<AppState>,
    Path((namespace, name, version)): Path<(String, String, String)>,
) -> Result<Json<CompatibilitySkillHit>, ApiError> {
    index_for_state_and_namespace(&state, Some(&namespace))
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
    let artifact_bytes = state
        .artifact_store
        .fetch_verified(&request.artifact)
        .await?;
    let derived_bundle_digest =
        bundle_digest_from_tar_zst(artifact_bytes.as_ref(), &request.manifest)?;
    if let Some(provided_bundle_digest) = request.bundle_digest.as_deref() {
        if provided_bundle_digest != derived_bundle_digest {
            return Err(ApiError::bad_request(format!(
                "bundle digest mismatch: expected `{provided_bundle_digest}`, found `{derived_bundle_digest}`"
            )));
        }
    }
    let mut request = request;
    request.bundle_digest = Some(derived_bundle_digest);
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
    filtered_search_index(
        &state,
        SearchParams {
            query: query.q.or(query.query),
            namespace: query.namespace,
            limit: query.limit,
        },
    )
    .await
    .map(Json)
}

pub async fn similar_search(
    State(state): State<AppState>,
    Json(request): Json<SimilarSearchRequest>,
) -> Result<Json<CompatibilityIndex>, ApiError> {
    let search = state
        .search
        .as_ref()
        .ok_or_else(|| ApiError::unavailable("semantic search is not configured"))?;
    let limit = request.limit.unwrap_or(20);
    let docs = search
        .similar(&request.embedding, limit)
        .await
        .map_err(|source| ApiError::internal(source.to_string()))?;
    Ok(Json(CompatibilityIndex {
        skills: docs
            .into_iter()
            .map(|doc| CompatibilitySkillHit {
                name: doc.name,
                version: doc.version,
                description: doc.description,
                registry: doc.namespace,
                digest: None,
                signature_ed25519: None,
                public_key_ed25519: None,
            })
            .collect(),
    }))
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
    State(state): State<AppState>,
    Path((name, artifact)): Path<(String, String)>,
) -> Result<Response, ApiError> {
    if let Some(repository) = &state.repository {
        return db_artifact_response(&state, repository, &name, &artifact).await;
    }

    if artifact.ends_with(".tar.zst") {
        let bytes = fixture_tarball_bytes()?;
        return Ok(([(header::CONTENT_TYPE, "application/octet-stream")], bytes).into_response());
    }
    if artifact.ends_with(".tar.zst.sig") {
        return Ok(fixture_signature().into_response());
    }

    Ok(StatusCode::NOT_FOUND.into_response())
}

async fn db_artifact_response(
    state: &AppState,
    repository: &hub_index::PgHubRepository,
    name: &str,
    artifact: &str,
) -> Result<Response, ApiError> {
    let Some((version, signature)) = artifact_version(artifact) else {
        return Ok(StatusCode::NOT_FOUND.into_response());
    };
    let record = repository.public_version_by_name(name, version).await?;
    if signature {
        let Some(signature) = record.signature_ed25519 else {
            return Ok(StatusCode::NOT_FOUND.into_response());
        };
        return Ok(format!("{signature}\n").into_response());
    }

    let bytes = state
        .artifact_store
        .fetch_verified(&ArtifactPointer {
            url: record.artifact_url,
            media_type: record.artifact_media_type.clone(),
            digest: record.artifact_digest,
        })
        .await?;
    Ok((
        [(header::CONTENT_TYPE, record.artifact_media_type)],
        bytes.to_vec(),
    )
        .into_response())
}

fn artifact_version(artifact: &str) -> Option<(&str, bool)> {
    artifact
        .strip_suffix(".tar.zst.sig")
        .map(|version| (version, true))
        .or_else(|| {
            artifact
                .strip_suffix(".tar.zst")
                .map(|version| (version, false))
        })
}

fn bundle_digest_from_tar_zst(bytes: &[u8], manifest: &SkillManifest) -> Result<String, ApiError> {
    let tar_bytes = zstd::stream::decode_all(Cursor::new(bytes)).map_err(|source| {
        ApiError::bad_request(format!(
            "skill artifact is not a valid zstd tarball: {source}"
        ))
    })?;
    let wanted_paths = manifest
        .files
        .iter()
        .map(|path| canonical_skill_path(path))
        .collect::<Result<BTreeSet<_>, _>>()?;
    let mut file_bytes = BTreeMap::new();
    let mut archive = tar::Archive::new(Cursor::new(tar_bytes));
    for entry in archive.entries().map_err(|source| {
        ApiError::bad_request(format!(
            "failed to read skill artifact tar entries: {source}"
        ))
    })? {
        let mut entry = entry.map_err(|source| {
            ApiError::bad_request(format!("failed to read skill artifact tar entry: {source}"))
        })?;
        let entry_path = entry.path().map_err(|source| {
            ApiError::bad_request(format!("failed to read skill artifact tar path: {source}"))
        })?;
        let entry_path = canonical_archive_path(&entry_path)?;
        let entry_type = entry.header().entry_type();
        if entry_type.is_dir() {
            continue;
        }
        if !entry_type.is_file() {
            return Err(ApiError::bad_request(format!(
                "skill artifact contains unsupported tar entry `{entry_path}`"
            )));
        }
        if wanted_paths.contains(&entry_path) {
            let mut content = Vec::new();
            entry.read_to_end(&mut content).map_err(|source| {
                ApiError::bad_request(format!(
                    "failed to read skill artifact file `{entry_path}`: {source}"
                ))
            })?;
            file_bytes.insert(entry_path, content);
        }
    }

    let mut hasher = Sha256::new();
    hasher.update(b"agentenv-skill-v1\n");
    for path in wanted_paths {
        let content = file_bytes.get(&path).ok_or_else(|| {
            ApiError::bad_request(format!("skill artifact is missing declared file `{path}`"))
        })?;
        hasher.update(path.as_bytes());
        hasher.update([0]);
        hasher.update(content.len().to_string().as_bytes());
        hasher.update([0]);
        hasher.update(content);
        hasher.update(b"\n");
    }
    Ok(format!("sha256:{:x}", hasher.finalize()))
}

fn canonical_archive_path(path: &FsPath) -> Result<String, ApiError> {
    let path = path
        .to_str()
        .ok_or_else(|| ApiError::bad_request("skill artifact contains a non-UTF-8 tar path"))?;
    canonical_skill_path(path)
}

fn canonical_skill_path(path: &str) -> Result<String, ApiError> {
    let path = validate_skill_path(path)?;
    canonical_path_string(&path)
}

fn canonical_path_string(path: &FsPath) -> Result<String, ApiError> {
    let mut parts = Vec::new();
    for component in path.components() {
        let std::path::Component::Normal(part) = component else {
            return Err(ApiError::bad_request(format!(
                "unsafe skill path `{}`",
                path.display()
            )));
        };
        let part = part.to_str().ok_or_else(|| {
            ApiError::bad_request(format!("skill path `{}` is not UTF-8", path.display()))
        })?;
        parts.push(part);
    }
    Ok(parts.join("/"))
}

pub async fn metrics() -> &'static str {
    "# HELP agentenv_skills_hub_info Skill hub info\n# TYPE agentenv_skills_hub_info gauge\nagentenv_skills_hub_info 1\n"
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
