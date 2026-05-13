use hub_core::model::{
    ArtifactPointer, CompatibilityIndex, PublishSkillRequest, SkillManifest, Visibility,
};
use hub_index::{run_migrations, PgHubRepository};
use sqlx::PgPool;

static TEST_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

fn database_url() -> String {
    std::env::var("DATABASE_URL").expect("DATABASE_URL must point at a test Postgres database")
}

fn signed_request(name: &str, version: &str, digest: &str) -> PublishSkillRequest {
    PublishSkillRequest {
        manifest: SkillManifest {
            name: name.to_owned(),
            version: version.to_owned(),
            description: Some("Review code changes".to_owned()),
            entry: "SKILL.md".to_owned(),
            files: vec!["SKILL.md".to_owned()],
        },
        artifact: ArtifactPointer {
            url: format!("file:///tmp/{name}-{version}.tar.zst"),
            media_type: "application/vnd.agentenv.skill.v1+tar".to_owned(),
            digest: digest.to_owned(),
        },
        signature_ed25519: Some("aa".repeat(64)),
        public_key_ed25519: Some("bb".repeat(32)),
        sigstore_bundle: None,
    }
}

#[tokio::test]
async fn repository_inserts_versions_and_builds_compatibility_index() {
    let _guard = TEST_LOCK.lock().await;
    let pool = PgPool::connect(&database_url()).await.unwrap();
    run_migrations(&pool).await.unwrap();
    sqlx::query("TRUNCATE webhook_deliveries, webhook_subscriptions, api_tokens, permissions, skill_embeddings, skill_versions, skills RESTART IDENTITY CASCADE")
        .execute(&pool)
        .await
        .unwrap();

    let repo = PgHubRepository::new(pool);
    let digest = "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    let record = repo
        .insert_version(
            "community",
            Visibility::Public,
            "alice",
            &signed_request("code-review", "1.2.0", digest),
        )
        .await
        .unwrap();

    assert_eq!(record.namespace, "community");
    assert_eq!(record.name, "code-review");
    assert_eq!(record.version, "1.2.0");

    let index: CompatibilityIndex = repo.compatibility_index("community").await.unwrap();
    assert_eq!(index.skills.len(), 1);
    assert_eq!(index.skills[0].name, "code-review");
    assert_eq!(index.skills[0].digest.as_deref(), Some(digest));
}

#[tokio::test]
async fn repository_yank_removes_version_from_compatibility_index() {
    let _guard = TEST_LOCK.lock().await;
    let pool = PgPool::connect(&database_url()).await.unwrap();
    run_migrations(&pool).await.unwrap();
    sqlx::query("TRUNCATE webhook_deliveries, webhook_subscriptions, api_tokens, permissions, skill_embeddings, skill_versions, skills RESTART IDENTITY CASCADE")
        .execute(&pool)
        .await
        .unwrap();

    let repo = PgHubRepository::new(pool);
    let digest = "sha256:cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc";
    repo.insert_version(
        "community",
        Visibility::Public,
        "alice",
        &signed_request("code-review", "1.2.0", digest),
    )
    .await
    .unwrap();
    repo.yank_version("community", "code-review", "1.2.0", "bad release")
        .await
        .unwrap();

    let index = repo.compatibility_index("community").await.unwrap();
    assert!(index.skills.is_empty());
}
