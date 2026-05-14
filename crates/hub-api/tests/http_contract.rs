use axum::{
    body::{to_bytes, Body},
    http::{header, Request, StatusCode},
};
use hub_api::{
    routes::{build_router, build_router_with_state},
    state::{AppState, RuntimeArtifactStore, RuntimeTrustVerifier, RuntimeWebhookQueue},
};
use hub_core::model::{ArtifactPointer, CompatibilityIndex, PublishSkillRequest, SkillManifest};
use hub_index::{run_migrations, PgHubRepository};
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use std::{
    fs,
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};
use tower::ServiceExt;

fn database_url() -> String {
    std::env::var("DATABASE_URL").expect("DATABASE_URL must point at a test Postgres database")
}

#[tokio::test]
async fn healthz_returns_ok_body() {
    let app = build_router();
    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/healthz")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(json_body(response).await["status"], "ok");
}

#[tokio::test]
async fn well_known_agent_skills_returns_discovery_document() {
    let app = build_router();
    let response = app
        .oneshot(
            Request::builder()
                .uri("/.well-known/agent-skills")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    assert_eq!(body["registry"]["index"], "/index.json");
    assert_eq!(body["registry"]["api"], "/api/v1");
}

#[tokio::test]
async fn index_json_digest_matches_served_fixture_artifact() {
    let app = build_router();
    let artifact_response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/skills/code-review/1.2.0.tar.zst")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(artifact_response.status(), StatusCode::OK);
    let artifact = to_bytes(artifact_response.into_body(), usize::MAX)
        .await
        .unwrap();
    let digest = format!("sha256:{:x}", Sha256::digest(&artifact));

    let index_response = app
        .oneshot(
            Request::builder()
                .uri("/index.json")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(index_response.status(), StatusCode::OK);
    let index: CompatibilityIndex =
        serde_json::from_value(json_body(index_response).await).unwrap();
    assert_eq!(index.skills[0].digest.as_deref(), Some(digest.as_str()));
}

#[tokio::test]
async fn publish_yank_and_unyank_update_database_backed_index() {
    let pool = PgPool::connect(&database_url()).await.unwrap();
    run_migrations(&pool).await.unwrap();
    let namespace = unique_name("api");
    let skill = unique_name("api-review");
    let artifact = temp_artifact(&skill);
    let digest = file_digest(&artifact);
    let state = AppState::with_repository(
        PgHubRepository::new(pool),
        "community",
        true,
        RuntimeArtifactStore::default(),
        RuntimeTrustVerifier,
        RuntimeWebhookQueue::default(),
        None,
    );
    let app = build_router_with_state(state);

    let request = PublishSkillRequest {
        manifest: SkillManifest {
            name: skill.clone(),
            version: "1.2.0".to_owned(),
            description: Some("API publish test".to_owned()),
            entry: "SKILL.md".to_owned(),
            files: vec!["SKILL.md".to_owned()],
        },
        artifact: ArtifactPointer {
            url: format!("file://{}", artifact.display()),
            media_type: "application/vnd.agentenv.skill.v1+tar".to_owned(),
            digest: digest.clone(),
        },
        signature_ed25519: None,
        public_key_ed25519: None,
        sigstore_bundle: None,
    };

    let publish_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/v1/skills/{namespace}/{skill}/versions"))
                .header(header::CONTENT_TYPE, "application/json")
                .header("x-agentenv-subject", "alice")
                .header("x-agentenv-scopes", "skills:publish,skills:yank")
                .header("x-agentenv-roles", format!("{namespace}:publisher"))
                .body(Body::from(serde_json::to_vec(&request).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(publish_response.status(), StatusCode::CREATED);

    let index = compatibility_index(app.clone(), "/index.json").await;
    let published = index
        .skills
        .iter()
        .find(|hit| hit.name == skill)
        .expect("published skill appears in index");
    assert_eq!(published.digest.as_deref(), Some(digest.as_str()));

    let served_artifact = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/skills/{skill}/1.2.0.tar.zst"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(served_artifact.status(), StatusCode::OK);
    let served_bytes = to_bytes(served_artifact.into_body(), usize::MAX)
        .await
        .unwrap();
    assert_eq!(
        format!("sha256:{:x}", Sha256::digest(&served_bytes)),
        digest
    );

    let yank_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!(
                    "/api/v1/skills/{namespace}/{skill}/versions/1.2.0/yank"
                ))
                .header(header::CONTENT_TYPE, "application/json")
                .header("x-agentenv-subject", "alice")
                .header("x-agentenv-scopes", "skills:yank")
                .header("x-agentenv-roles", format!("{namespace}:publisher"))
                .body(Body::from(r#"{"reason":"bad release"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(yank_response.status(), StatusCode::NO_CONTENT);
    assert!(!compatibility_index(app.clone(), "/index.json")
        .await
        .skills
        .iter()
        .any(|hit| hit.name == skill));

    let unyank_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!(
                    "/api/v1/skills/{namespace}/{skill}/versions/1.2.0/unyank"
                ))
                .header("x-agentenv-subject", "alice")
                .header("x-agentenv-scopes", "skills:yank")
                .header("x-agentenv-roles", format!("{namespace}:publisher"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(unyank_response.status(), StatusCode::NO_CONTENT);
    assert_eq!(
        compatibility_index(app, &format!("/api/v1/search?q={skill}"))
            .await
            .skills
            .len(),
        1
    );
}

#[tokio::test]
async fn publish_without_auth_is_forbidden_not_healthz_stub() {
    let app = build_router();
    let request = publish_request(
        "code-review",
        "file:///tmp/code-review.tar.zst",
        "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
    );
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/skills/community/code-review/versions")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(serde_json::to_vec(&request).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

async fn compatibility_index(app: axum::Router, uri: &str) -> CompatibilityIndex {
    let response = app
        .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    serde_json::from_value(json_body(response).await).unwrap()
}

async fn json_body(response: axum::response::Response) -> serde_json::Value {
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    serde_json::from_slice(&body).unwrap()
}

fn unique_name(prefix: &str) -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    format!("{prefix}-{nanos}")
}

fn temp_artifact(skill: &str) -> std::path::PathBuf {
    let path = std::env::temp_dir().join(format!("{skill}.tar.zst"));
    fs::write(&path, b"api publish artifact").unwrap();
    path
}

fn file_digest(path: &Path) -> String {
    format!("sha256:{:x}", Sha256::digest(fs::read(path).unwrap()))
}

fn publish_request(skill: &str, url: &str, digest: &str) -> PublishSkillRequest {
    PublishSkillRequest {
        manifest: SkillManifest {
            name: skill.to_owned(),
            version: "1.2.0".to_owned(),
            description: Some("API publish test".to_owned()),
            entry: "SKILL.md".to_owned(),
            files: vec!["SKILL.md".to_owned()],
        },
        artifact: ArtifactPointer {
            url: url.to_owned(),
            media_type: "application/vnd.agentenv.skill.v1+tar".to_owned(),
            digest: digest.to_owned(),
        },
        signature_ed25519: None,
        public_key_ed25519: None,
        sigstore_bundle: None,
    }
}
