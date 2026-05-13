use async_trait::async_trait;
use bytes::Bytes;

use crate::{ArtifactStore, StorageError, StorageResult};

#[derive(Debug, Clone)]
pub struct S3ArtifactStore {
    client: reqwest::Client,
}

impl S3ArtifactStore {
    pub fn new() -> StorageResult<Self> {
        let client = reqwest::Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|source| StorageError::Client { source })?;

        Ok(Self { client })
    }
}

#[async_trait]
impl ArtifactStore for S3ArtifactStore {
    async fn fetch_verified(&self, pointer: &str, _expected_digest: &str) -> StorageResult<Bytes> {
        let _client = &self.client;

        Err(StorageError::InvalidPointer {
            value: pointer.to_owned(),
            message: "S3 artifact endpoints are not configured".to_owned(),
        })
    }
}
