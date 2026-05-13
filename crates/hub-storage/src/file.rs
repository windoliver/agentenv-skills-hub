use async_trait::async_trait;
use bytes::Bytes;
use sha2::{Digest, Sha256};
use tokio::fs;
use url::Url;

use crate::{pointer::parse_artifact_pointer, ArtifactStore, StorageError, StorageResult};

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

        let expected = normalize_sha256_digest(expected_digest)?;
        let path = file_path(&url, pointer)?;
        let bytes = fs::read(&path)
            .await
            .map_err(|source| StorageError::ReadArtifact {
                path: path.clone(),
                source,
            })?;
        let actual = sha256_digest(&bytes);

        if actual != expected {
            return Err(StorageError::DigestMismatch { expected, actual });
        }

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
