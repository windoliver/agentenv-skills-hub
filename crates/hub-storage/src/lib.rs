pub mod file;
pub mod oci;
pub mod pointer;
pub mod s3;

use async_trait::async_trait;
use bytes::Bytes;
use sha2::{Digest, Sha256};
use thiserror::Error;

pub use file::FileArtifactStore;
pub use oci::OciArtifactStore;
pub use s3::S3ArtifactStore;

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
    #[error("failed to fetch artifact pointer `{pointer}`: {source}")]
    Fetch {
        pointer: String,
        #[source]
        source: reqwest::Error,
    },
}

pub type StorageResult<T> = Result<T, StorageError>;

#[async_trait]
pub trait ArtifactStore {
    async fn fetch_verified(&self, pointer: &str, expected_digest: &str) -> StorageResult<Bytes>;
}

pub(crate) fn verify_sha256_digest(bytes: &[u8], expected_digest: &str) -> StorageResult<()> {
    let expected = normalize_sha256_digest(expected_digest)?;
    let actual = sha256_digest(bytes);

    if actual != expected {
        return Err(StorageError::DigestMismatch { expected, actual });
    }

    Ok(())
}

fn normalize_sha256_digest(value: &str) -> StorageResult<String> {
    let Some(hex) = value.strip_prefix("sha256:") else {
        return Err(StorageError::InvalidDigest {
            value: value.to_owned(),
        });
    };

    if hex.len() != 64 || !hex.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(StorageError::InvalidDigest {
            value: value.to_owned(),
        });
    }

    Ok(format!("sha256:{}", hex.to_ascii_lowercase()))
}

fn sha256_digest(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut hex = String::with_capacity(digest.len() * 2);

    for byte in digest {
        push_hex_byte(&mut hex, byte);
    }

    format!("sha256:{hex}")
}

fn push_hex_byte(hex: &mut String, byte: u8) {
    const TABLE: &[u8; 16] = b"0123456789abcdef";

    hex.push(TABLE[(byte >> 4) as usize] as char);
    hex.push(TABLE[(byte & 0x0f) as usize] as char);
}
