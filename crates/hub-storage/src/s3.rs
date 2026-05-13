use async_trait::async_trait;
use bytes::Bytes;
use url::Url;

use crate::{
    pointer::parse_artifact_pointer, verify_sha256_digest, ArtifactStore, StorageError,
    StorageResult,
};

#[derive(Debug, Clone)]
pub struct S3ArtifactStore {
    client: reqwest::Client,
    endpoint: Option<Url>,
}

impl S3ArtifactStore {
    pub fn new() -> StorageResult<Self> {
        Ok(Self {
            client: build_client()?,
            endpoint: None,
        })
    }

    pub fn new_for_endpoint(endpoint: &str) -> StorageResult<Self> {
        let mut endpoint = Url::parse(endpoint).map_err(|source| StorageError::InvalidPointer {
            value: endpoint.to_owned(),
            message: format!("invalid S3 endpoint: {source}"),
        })?;
        endpoint.set_query(None);
        endpoint.set_fragment(None);

        Ok(Self {
            client: build_client()?,
            endpoint: Some(endpoint),
        })
    }
}

#[async_trait]
impl ArtifactStore for S3ArtifactStore {
    async fn fetch_verified(&self, pointer: &str, expected_digest: &str) -> StorageResult<Bytes> {
        let endpoint = self
            .endpoint
            .as_ref()
            .ok_or_else(|| StorageError::InvalidPointer {
                value: pointer.to_owned(),
                message: "S3 artifact endpoints are not configured".to_owned(),
            })?;
        let url = parse_artifact_pointer(pointer)?;
        if url.scheme() != "s3" {
            return Err(StorageError::InvalidPointer {
                value: pointer.to_owned(),
                message: format!(
                    "S3 artifact store cannot fetch `{}` artifacts",
                    url.scheme()
                ),
            });
        }
        if url.query().is_some() || url.fragment().is_some() {
            return Err(StorageError::InvalidPointer {
                value: pointer.to_owned(),
                message: "S3 artifact pointer must not include query or fragment".to_owned(),
            });
        }

        let bucket = url.host_str().ok_or_else(|| StorageError::InvalidPointer {
            value: pointer.to_owned(),
            message: "S3 artifact pointer must include a bucket host".to_owned(),
        })?;
        let key = url.path().trim_start_matches('/');
        if key.is_empty() {
            return Err(StorageError::InvalidPointer {
                value: pointer.to_owned(),
                message: "S3 artifact pointer must include an object key".to_owned(),
            });
        }

        let fetch_url = endpoint_url(endpoint, &format!("{bucket}/{key}"));
        let bytes = self
            .client
            .get(fetch_url)
            .send()
            .await
            .map_err(|source| StorageError::Fetch {
                pointer: pointer.to_owned(),
                source,
            })?
            .error_for_status()
            .map_err(|source| StorageError::Fetch {
                pointer: pointer.to_owned(),
                source,
            })?
            .bytes()
            .await
            .map_err(|source| StorageError::Fetch {
                pointer: pointer.to_owned(),
                source,
            })?;

        verify_sha256_digest(&bytes, expected_digest)?;

        Ok(bytes)
    }
}

fn build_client() -> StorageResult<reqwest::Client> {
    let client = reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|source| StorageError::Client { source })?;

    Ok(client)
}

fn endpoint_url(endpoint: &Url, suffix: &str) -> Url {
    let mut url = endpoint.clone();
    let base_path = url.path().trim_end_matches('/');
    url.set_path(&format!("{base_path}/{suffix}"));

    url
}
