use axum::{
    extract::Path,
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use hub_core::model::{CompatibilityIndex, CompatibilitySkillHit};
use serde_json::json;
use std::{io::Write, path::PathBuf};

pub async fn healthz() -> Json<serde_json::Value> {
    Json(json!({"status": "ok"}))
}

pub async fn readyz() -> Json<serde_json::Value> {
    Json(json!({"status": "ready"}))
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

pub async fn index_json() -> Json<CompatibilityIndex> {
    Json(CompatibilityIndex {
        skills: vec![CompatibilitySkillHit {
            name: "code-review".to_owned(),
            version: "1.2.0".to_owned(),
            description: Some("Review code changes".to_owned()),
            registry: "community".to_owned(),
            digest: Some(
                "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
                    .to_owned(),
            ),
            signature_ed25519: Some("aa".repeat(64)),
            public_key_ed25519: Some("bb".repeat(32)),
        }],
    })
}

pub async fn fixture_artifact(Path((_name, artifact)): Path<(String, String)>) -> Response {
    if artifact.ends_with(".tar.zst") {
        return fixture_tarball().into_response();
    }
    if artifact.ends_with(".tar.zst.sig") {
        return fixture_signature().into_response();
    }

    StatusCode::NOT_FOUND.into_response()
}

fn fixture_tarball() -> impl IntoResponse {
    let mut tar_bytes = Vec::new();
    {
        let encoder = zstd::stream::write::Encoder::new(&mut tar_bytes, 0).expect("zstd encoder");
        let mut builder = tar::Builder::new(encoder);
        append_bytes(
            &mut builder,
            "skill.yaml",
            include_bytes!("../../../tests/fixtures/code-review-skill/skill.yaml"),
        );
        append_bytes(
            &mut builder,
            "SKILL.md",
            include_bytes!("../../../tests/fixtures/code-review-skill/SKILL.md"),
        );
        let encoder = builder.into_inner().expect("finish tar");
        encoder.finish().expect("finish zstd");
    }

    (
        [(header::CONTENT_TYPE, "application/octet-stream")],
        tar_bytes,
    )
}

fn fixture_signature() -> &'static str {
    "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\n"
}

pub async fn metrics() -> &'static str {
    "# HELP agentenv_skills_hub_info Skill hub info\n# TYPE agentenv_skills_hub_info gauge\nagentenv_skills_hub_info 1\n"
}

fn append_bytes<W: Write>(builder: &mut tar::Builder<W>, path: &str, bytes: &[u8]) {
    let mut header = tar::Header::new_gnu();
    header.set_size(bytes.len() as u64);
    header.set_mode(0o644);
    header.set_cksum();
    builder
        .append_data(&mut header, PathBuf::from(path), bytes)
        .expect("append fixture file");
}
