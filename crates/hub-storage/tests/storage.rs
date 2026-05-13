use hub_storage::{ArtifactStore, FileArtifactStore};
use sha2::{Digest, Sha256};
use std::path::PathBuf;
use tokio::fs;

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
