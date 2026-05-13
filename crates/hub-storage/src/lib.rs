pub mod file;
pub mod oci;
pub mod pointer;
pub mod s3;

use async_trait::async_trait;
use bytes::Bytes;
use thiserror::Error;

pub use file::FileArtifactStore;

#[derive(Debug, Error)]
pub enum StorageError {
    #[error("invalid artifact pointer `{value}`: {message}")]
    InvalidPointer { value: String, message: String },
    #[error("invalid digest `{value}`")]
    InvalidDigest { value: String },
    #[error("digest mismatch: expected `{expected}`, got `{actual}`")]
    DigestMismatch { expected: String, actual: String },
    #[error("failed to read artifact `{path}`: {source}")]
    ReadArtifact {
        path: std::path::PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to create storage client: {source}")]
    Client {
        #[source]
        source: reqwest::Error,
    },
}

pub type StorageResult<T> = Result<T, StorageError>;

#[async_trait]
pub trait ArtifactStore {
    async fn fetch_verified(&self, pointer: &str, expected_digest: &str) -> StorageResult<Bytes>;
}
