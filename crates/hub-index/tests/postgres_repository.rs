use hub_core::{
    error::HubError,
    model::{ArtifactPointer, CompatibilityIndex, PublishSkillRequest, SkillManifest, Visibility},
};
use hub_index::{run_migrations, PgHubRepository};
use sqlx::PgPool;

static TEST_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

fn database_url() -> String {
    std::env::var("DATABASE_URL").expect("DATABASE_URL must point at a test Postgres database")
}

fn signed_request(name: &str, version: &str, digest: &str) -> PublishSkillRequest {
    signed_request_with_description(name, version, digest, Some("Review code changes"))
}

fn signed_request_with_description(
    name: &str,
    version: &str,
    digest: &str,
    description: Option<&str>,
) -> PublishSkillRequest {
    PublishSkillRequest {
        manifest: SkillManifest {
            name: name.to_owned(),
            version: version.to_owned(),
            description: description.map(str::to_owned),
            entry: "SKILL.md".to_owned(),
            files: vec!["SKILL.md".to_owned()],
        },
        artifact: ArtifactPointer {
            url: format!("file:///tmp/{name}-{version}.tar.zst"),
            media_type: "application/vnd.agentenv.skill.v1+tar".to_owned(),
            digest: digest.to_owned(),
        },
        bundle_digest: None,
        signature_ed25519: Some("aa".repeat(64)),
        public_key_ed25519: Some("bb".repeat(32)),
        sigstore_bundle: None,
    }
}

async fn reset_database(pool: &PgPool) {
    run_migrations(pool).await.unwrap();
    sqlx::query("TRUNCATE webhook_deliveries, webhook_subscriptions, api_tokens, permissions, skill_embeddings, skill_versions, skills RESTART IDENTITY CASCADE")
        .execute(pool)
        .await
        .unwrap();
}

#[tokio::test]
async fn repository_inserts_versions_and_builds_compatibility_index() {
    let _guard = TEST_LOCK.lock().await;
    let pool = PgPool::connect(&database_url()).await.unwrap();
    reset_database(&pool).await;

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
async fn repository_returns_persisted_record_for_duplicate_same_digest_publish() {
    let _guard = TEST_LOCK.lock().await;
    let pool = PgPool::connect(&database_url()).await.unwrap();
    reset_database(&pool).await;

    let repo = PgHubRepository::new(pool);
    let digest = "sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
    let request = signed_request("code-review", "1.2.0", digest);
    let first = repo
        .insert_version("community", Visibility::Public, "alice", &request)
        .await
        .unwrap();
    let second = repo
        .insert_version("community", Visibility::Public, "alice", &request)
        .await
        .unwrap();

    assert_eq!(second.id, first.id);
    assert_eq!(second.digest, digest);

    let index = repo.compatibility_index("community").await.unwrap();
    assert_eq!(index.skills.len(), 1);
    assert_eq!(index.skills[0].digest.as_deref(), Some(digest));
}

#[tokio::test]
async fn repository_rejects_duplicate_publish_with_different_digest() {
    let _guard = TEST_LOCK.lock().await;
    let pool = PgPool::connect(&database_url()).await.unwrap();
    reset_database(&pool).await;

    let repo = PgHubRepository::new(pool);
    let first_digest = "sha256:dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd";
    let second_digest = "sha256:eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee";
    repo.insert_version(
        "community",
        Visibility::Public,
        "alice",
        &signed_request("code-review", "1.2.0", first_digest),
    )
    .await
    .unwrap();

    let error = repo
        .insert_version(
            "community",
            Visibility::Public,
            "alice",
            &signed_request("code-review", "1.2.0", second_digest),
        )
        .await
        .unwrap_err();

    assert!(matches!(
        error,
        HubError::VersionDigestConflict {
            namespace,
            name,
            version
        } if namespace == "community" && name == "code-review" && version == "1.2.0"
    ));
}

#[tokio::test]
async fn repository_handles_concurrent_duplicate_same_digest_publish() {
    let _guard = TEST_LOCK.lock().await;
    let pool = PgPool::connect(&database_url()).await.unwrap();
    reset_database(&pool).await;

    let repo = PgHubRepository::new(pool);
    let digest = "sha256:ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff";
    let request = signed_request("code-review", "1.2.0", digest);
    let first_repo = repo.clone();
    let second_repo = repo.clone();
    let first_request = request.clone();
    let second_request = request;

    let (first, second) = tokio::join!(
        async move {
            first_repo
                .insert_version("community", Visibility::Public, "alice", &first_request)
                .await
        },
        async move {
            second_repo
                .insert_version("community", Visibility::Public, "alice", &second_request)
                .await
        }
    );
    let first = first.unwrap();
    let second = second.unwrap();

    assert_eq!(first.id, second.id);
    let index = repo.compatibility_index("community").await.unwrap();
    assert_eq!(index.skills.len(), 1);
    assert_eq!(index.skills[0].digest.as_deref(), Some(digest));
}

#[tokio::test]
async fn repository_yank_and_unyank_toggle_compatibility_index_visibility() {
    let _guard = TEST_LOCK.lock().await;
    let pool = PgPool::connect(&database_url()).await.unwrap();
    reset_database(&pool).await;

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

    repo.unyank_version("community", "code-review", "1.2.0")
        .await
        .unwrap();
    let index = repo.compatibility_index("community").await.unwrap();
    assert_eq!(index.skills.len(), 1);
    assert_eq!(index.skills[0].digest.as_deref(), Some(digest));
}

#[tokio::test]
async fn compatibility_index_uses_version_descriptions_and_semver_ordering() {
    let _guard = TEST_LOCK.lock().await;
    let pool = PgPool::connect(&database_url()).await.unwrap();
    reset_database(&pool).await;

    let repo = PgHubRepository::new(pool);
    repo.insert_version(
        "community",
        Visibility::Public,
        "alice",
        &signed_request_with_description(
            "code-review",
            "1.10.0",
            "sha256:1111111111111111111111111111111111111111111111111111111111111111",
            Some("Larger release"),
        ),
    )
    .await
    .unwrap();
    repo.insert_version(
        "community",
        Visibility::Public,
        "alice",
        &signed_request_with_description(
            "code-review",
            "1.2.0",
            "sha256:2222222222222222222222222222222222222222222222222222222222222222",
            Some("Smaller release"),
        ),
    )
    .await
    .unwrap();

    let index = repo.compatibility_index("community").await.unwrap();
    let versions = index
        .skills
        .iter()
        .map(|hit| (hit.version.as_str(), hit.description.as_deref()))
        .collect::<Vec<_>>();

    assert_eq!(
        versions,
        vec![
            ("1.2.0", Some("Smaller release")),
            ("1.10.0", Some("Larger release"))
        ]
    );
}
