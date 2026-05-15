use axum::http::StatusCode;
use hub_core::model::{CompatibilityIndex, CompatibilitySkillHit, SkillManifest};
use hub_search::{lexical_rank, SearchDocument};
use sha2::{Digest, Sha256};
use std::{io::Write, path::PathBuf};

use crate::{error::ApiError, state::AppState};

#[derive(Debug, Clone, Default)]
pub(crate) struct SearchParams {
    pub query: Option<String>,
    pub namespace: Option<String>,
    pub limit: Option<usize>,
}

pub(crate) async fn filtered_search_index(
    state: &AppState,
    params: SearchParams,
) -> Result<CompatibilityIndex, ApiError> {
    let mut index = index_for_state_and_namespace(state, params.namespace.as_deref()).await?;
    let Some(search_terms) = params.query else {
        truncate_index(&mut index, params.limit);
        return Ok(index);
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
    if let Some(limit) = params.limit {
        skills.truncate(limit);
    }
    Ok(CompatibilityIndex { skills })
}

pub(crate) async fn index_for_state(state: &AppState) -> Result<CompatibilityIndex, ApiError> {
    index_for_state_and_namespace(state, None).await
}

pub(crate) async fn index_for_state_and_namespace(
    state: &AppState,
    namespace: Option<&str>,
) -> Result<CompatibilityIndex, ApiError> {
    if let Some(repository) = &state.repository {
        if let Some(namespace) = namespace {
            return Ok(repository
                .compatibility_index_for_namespace(namespace)
                .await?);
        }
        return Ok(repository.compatibility_index(&state.registry).await?);
    }
    let index = fixture_index(&state.registry)?;
    if let Some(namespace) = namespace {
        return Ok(filter_index(index, |hit| hit.registry == namespace));
    }
    Ok(index)
}

pub(crate) fn fixture_index(registry: &str) -> Result<CompatibilityIndex, ApiError> {
    Ok(CompatibilityIndex {
        skills: vec![CompatibilitySkillHit {
            name: "code-review".to_owned(),
            version: "1.2.0".to_owned(),
            description: Some("Review code changes".to_owned()),
            registry: registry.to_owned(),
            digest: Some(fixture_bundle_digest()),
            signature_ed25519: None,
            public_key_ed25519: None,
        }],
    })
}

pub(crate) fn filter_index(
    index: CompatibilityIndex,
    predicate: impl Fn(&CompatibilitySkillHit) -> bool,
) -> CompatibilityIndex {
    CompatibilityIndex {
        skills: index.skills.into_iter().filter(predicate).collect(),
    }
}

pub(crate) fn truncate_index(index: &mut CompatibilityIndex, limit: Option<usize>) {
    if let Some(limit) = limit {
        index.skills.truncate(limit);
    }
}

pub(crate) async fn manifest_for_state(
    state: &AppState,
    name: &str,
    version: Option<&str>,
) -> Result<SkillManifest, ApiError> {
    if let Some(repository) = &state.repository {
        return Ok(repository.public_manifest_by_name(name, version).await?);
    }

    let manifest = fixture_manifest();
    let version_matches = match version {
        Some(version) => version == manifest.version,
        None => true,
    };
    if name == manifest.name && version_matches {
        return Ok(manifest);
    }

    Err(ApiError {
        status: StatusCode::NOT_FOUND,
        message: "skill manifest was not found".to_owned(),
    })
}

pub(crate) fn fixture_manifest() -> SkillManifest {
    SkillManifest {
        name: "code-review".to_owned(),
        version: "1.2.0".to_owned(),
        description: Some("Review code changes".to_owned()),
        entry: "SKILL.md".to_owned(),
        files: vec!["SKILL.md".to_owned()],
    }
}

pub(crate) fn fixture_tarball_bytes() -> Result<Vec<u8>, ApiError> {
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

pub(crate) fn fixture_signature() -> &'static str {
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

fn fixture_bundle_digest() -> String {
    let content = include_bytes!("../../../tests/fixtures/code-review-skill/SKILL.md");
    let mut hasher = Sha256::new();
    hasher.update(b"agentenv-skill-v1\n");
    hasher.update(b"SKILL.md");
    hasher.update([0]);
    hasher.update(content.len().to_string().as_bytes());
    hasher.update([0]);
    hasher.update(content);
    hasher.update(b"\n");
    format!("sha256:{:x}", hasher.finalize())
}
