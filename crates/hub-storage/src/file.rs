use async_trait::async_trait;
use bytes::Bytes;
use tokio::fs;
use url::Url;

use crate::{
    pointer::parse_artifact_pointer, verify_sha256_digest, ArtifactStore, StorageError,
    StorageResult,
};

#[derive(Debug, Default, Clone, Copy)]
pub struct FileArtifactStore;

#[async_trait]
impl ArtifactStore for FileArtifactStore {
    async fn fetch_verified(&self, pointer: &str, expected_digest: &str) -> StorageResult<Bytes> {
        let url = parse_artifact_pointer(pointer)?;
        if url.scheme() != "file" {
            return Err(StorageError::InvalidPointer {
                value: pointer.to_owned(),
                message: format!(
                    "file artifact store cannot fetch `{}` artifacts",
                    url.scheme()
                ),
            });
        }

        let path = file_path(&url, pointer)?;
        let bytes = fs::read(&path)
            .await
            .map_err(|source| StorageError::ReadArtifact {
                path: path.clone(),
                source,
            })?;
        verify_sha256_digest(&bytes, expected_digest)?;

        Ok(Bytes::from(bytes))
    }
}

fn file_path(url: &Url, pointer: &str) -> StorageResult<std::path::PathBuf> {
    url.to_file_path()
        .map_err(|()| StorageError::InvalidPointer {
            value: pointer.to_owned(),
            message: "file artifact pointer is not a valid local path".to_owned(),
        })
}
