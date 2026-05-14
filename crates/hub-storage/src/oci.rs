use async_trait::async_trait;
use bytes::Bytes;
use serde::Deserialize;
use url::Url;

use crate::{
    pointer::parse_artifact_pointer, verify_sha256_digest, ArtifactStore, StorageError,
    StorageResult,
};

#[derive(Debug, Clone)]
pub struct OciArtifactStore {
    client: reqwest::Client,
    registry_host: Option<String>,
    base_url: Option<Url>,
}

impl OciArtifactStore {
    pub fn new() -> StorageResult<Self> {
        Ok(Self {
            client: build_client()?,
            registry_host: None,
            base_url: None,
        })
    }

    pub fn new_for_registry(registry_host: &str, base_url: &str) -> StorageResult<Self> {
        let mut base_url = Url::parse(base_url).map_err(|source| StorageError::InvalidPointer {
            value: base_url.to_owned(),
            message: format!("invalid OCI registry endpoint: {source}"),
        })?;
        base_url.set_query(None);
        base_url.set_fragment(None);

        Ok(Self {
            client: build_client()?,
            registry_host: Some(registry_host.to_owned()),
            base_url: Some(base_url),
        })
    }
}

#[async_trait]
impl ArtifactStore for OciArtifactStore {
    async fn fetch_verified(&self, pointer: &str, expected_digest: &str) -> StorageResult<Bytes> {
        let registry_host =
            self.registry_host
                .as_deref()
                .ok_or_else(|| StorageError::InvalidPointer {
                    value: pointer.to_owned(),
                    message: "OCI artifact endpoints are not configured".to_owned(),
                })?;
        let base_url = self
            .base_url
            .as_ref()
            .ok_or_else(|| StorageError::InvalidPointer {
                value: pointer.to_owned(),
                message: "OCI artifact endpoints are not configured".to_owned(),
            })?;
        let url = parse_artifact_pointer(pointer)?;
        if url.scheme() != "oci" {
            return Err(StorageError::InvalidPointer {
                value: pointer.to_owned(),
                message: format!(
                    "OCI artifact store cannot fetch `{}` artifacts",
                    url.scheme()
                ),
            });
        }
        if url.query().is_some() || url.fragment().is_some() {
            return Err(StorageError::InvalidPointer {
                value: pointer.to_owned(),
                message: "OCI artifact pointer must not include query or fragment".to_owned(),
            });
        }
        if url.host_str() != Some(registry_host) {
            return Err(StorageError::InvalidPointer {
                value: pointer.to_owned(),
                message: format!("OCI registry is not configured for `{pointer}`"),
            });
        }

        let image = oci_image(pointer, &url)?;
        let manifest_url = endpoint_url(
            base_url,
            &format!("v2/{}/manifests/{}", image.repo, image.tag),
        );
        let manifest = self
            .client
            .get(manifest_url)
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
            .json::<OciManifest>()
            .await
            .map_err(|source| StorageError::Fetch {
                pointer: pointer.to_owned(),
                source,
            })?;

        let layer_digest = first_layer_digest(pointer, manifest, expected_digest)?;
        let layer_url = endpoint_url(
            base_url,
            &format!("v2/{}/blobs/{}", image.repo, layer_digest),
        );
        let bytes = self
            .client
            .get(layer_url)
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

fn endpoint_url(base_url: &Url, suffix: &str) -> Url {
    let mut url = base_url.clone();
    let base_path = url.path().trim_end_matches('/');
    url.set_path(&format!("{base_path}/{suffix}"));

    url
}

fn oci_image<'a>(pointer: &str, url: &'a Url) -> StorageResult<OciImage<'a>> {
    let path = url.path().trim_start_matches('/');
    let Some((repo, tag)) = path.rsplit_once(':') else {
        return Err(StorageError::InvalidPointer {
            value: pointer.to_owned(),
            message: "OCI artifact pointer path must include repo:tag".to_owned(),
        });
    };

    if repo.is_empty() || tag.is_empty() {
        return Err(StorageError::InvalidPointer {
            value: pointer.to_owned(),
            message: "OCI artifact pointer path must include repo:tag".to_owned(),
        });
    }

    Ok(OciImage { repo, tag })
}

fn first_layer_digest(
    pointer: &str,
    manifest: OciManifest,
    expected_digest: &str,
) -> StorageResult<String> {
    let digest = manifest
        .layers
        .into_iter()
        .map(|layer| layer.digest)
        .find(|digest| digest == expected_digest)
        .ok_or_else(|| StorageError::InvalidPointer {
            value: pointer.to_owned(),
            message: "OCI manifest does not include the expected layer digest".to_owned(),
        })?;
    if digest.contains('/') || digest.contains('?') || digest.contains('#') {
        return Err(StorageError::InvalidPointer {
            value: pointer.to_owned(),
            message: "OCI layer digest contains invalid URL characters".to_owned(),
        });
    }
    Ok(digest)
}

struct OciImage<'a> {
    repo: &'a str,
    tag: &'a str,
}

#[derive(Debug, Deserialize)]
struct OciManifest {
    layers: Vec<OciLayer>,
}

#[derive(Debug, Deserialize)]
struct OciLayer {
    digest: String,
}
