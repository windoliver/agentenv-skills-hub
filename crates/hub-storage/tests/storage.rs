use axum::{extract::State, http::header::CONTENT_TYPE, routing::get, Router};
use bytes::Bytes;
use hub_storage::{ArtifactStore, FileArtifactStore, OciArtifactStore, S3ArtifactStore};
use sha2::{Digest, Sha256};
use std::net::SocketAddr;
use std::path::PathBuf;
use tokio::{fs, net::TcpListener};

fn test_path(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!("hub-storage-{}-{}", std::process::id(), name))
}

fn sha256_digest(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut hex = String::with_capacity(digest.len() * 2);

    for byte in digest {
        hex.push_str(&format!("{byte:02x}"));
    }

    format!("sha256:{hex}")
}

#[tokio::test]
async fn file_artifact_store_fetch_verified_reads_file_artifact_and_verifies_digest() {
    let artifact = b"verified artifact bytes";
    let path = test_path("verified.txt");
    fs::write(&path, artifact)
        .await
        .expect("write test artifact");

    let url = url::Url::from_file_path(&path).expect("file url");
    let store = FileArtifactStore::default();

    let fetched = store
        .fetch_verified(url.as_str(), &sha256_digest(artifact))
        .await
        .expect("verified fetch succeeds");

    assert_eq!(fetched.as_ref(), artifact);

    fs::remove_file(path).await.expect("remove test artifact");
}

#[tokio::test]
async fn file_artifact_store_fetch_verified_returns_digest_mismatch_error() {
    let artifact = b"artifact with wrong digest";
    let path = test_path("mismatch.txt");
    fs::write(&path, artifact)
        .await
        .expect("write test artifact");

    let url = url::Url::from_file_path(&path).expect("file url");
    let store = FileArtifactStore::default();

    let error = store
        .fetch_verified(url.as_str(), &sha256_digest(b"different bytes"))
        .await
        .expect_err("digest mismatch fails");

    assert!(error.to_string().contains("digest mismatch"));

    fs::remove_file(path).await.expect("remove test artifact");
}

#[tokio::test]
async fn s3_store_fetches_pointer_from_configured_endpoint_and_verifies_digest() {
    let artifact = Bytes::from_static(b"remote s3 artifact bytes");
    let base = spawn_artifact_server(artifact.clone()).await;
    let store = S3ArtifactStore::new_for_endpoint(&base).expect("s3 store");

    let fetched = store
        .fetch_verified(
            "s3://agentenv-skills/code-review/1.2.0.tar.zst",
            &sha256_digest(&artifact),
        )
        .await
        .expect("verified fetch succeeds");

    assert_eq!(fetched, artifact);
}

#[tokio::test]
async fn oci_store_fetches_pointer_from_configured_registry_and_verifies_digest() {
    let artifact = Bytes::from_static(b"remote oci artifact bytes");
    let base = spawn_artifact_server(artifact.clone()).await;
    let store = OciArtifactStore::new_for_registry("127.0.0.1", &base).expect("oci store");

    let fetched = store
        .fetch_verified(
            "oci://127.0.0.1/acme/skills:1.2.0",
            &sha256_digest(&artifact),
        )
        .await
        .expect("verified fetch succeeds");

    assert_eq!(fetched, artifact);
}

async fn spawn_artifact_server(artifact: Bytes) -> String {
    let app = Router::new()
        .route("/v2/acme/skills/manifests/1.2.0", get(oci_manifest))
        .route("/blob", get(artifact_blob))
        .route(
            "/agentenv-skills/code-review/1.2.0.tar.zst",
            get(artifact_blob),
        )
        .with_state(artifact);
    let listener = TcpListener::bind(SocketAddr::from(([127, 0, 0, 1], 0)))
        .await
        .expect("bind artifact server");
    let address = listener.local_addr().expect("artifact server address");

    tokio::spawn(async move {
        axum::serve(listener, app).await.expect("artifact server");
    });

    format!("http://{address}")
}

async fn oci_manifest() -> ([(&'static str, &'static str); 1], &'static str) {
    (
        [(CONTENT_TYPE.as_str(), "application/json")],
        r#"{"layers":[{"urls":["/blob"]}]}"#,
    )
}

async fn artifact_blob(State(artifact): State<Bytes>) -> Bytes {
    artifact
}
