use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use bytes::Bytes;
use hub_attestation::{verify_ed25519, verify_sigstore_bundle};
use hub_core::{
    error::{HubError, HubResult},
    model::{ArtifactPointer, PublishSkillRequest},
    service::{HubService, TrustVerifier, VerifiedArtifactStore, WebhookEvent, WebhookQueue},
};
use hub_index::{run_migrations, PgHubRepository};
use hub_search::pgvector::PgVectorSearch;
use hub_storage::{
    pointer::parse_artifact_pointer, ArtifactStore, FileArtifactStore, OciArtifactStore,
    S3ArtifactStore, StorageError,
};
use sqlx::PgPool;

pub type RuntimeHubService =
    HubService<PgHubRepository, RuntimeArtifactStore, RuntimeTrustVerifier, RuntimeWebhookQueue>;

#[derive(Clone)]
pub struct AppState {
    pub registry: String,
    pub repository: Option<PgHubRepository>,
    pub service: Option<RuntimeHubService>,
    pub artifact_store: RuntimeArtifactStore,
    pub search: Option<PgVectorSearch>,
}

impl AppState {
    pub fn fixture() -> Self {
        Self {
            registry: "community".to_owned(),
            repository: None,
            service: None,
            artifact_store: RuntimeArtifactStore::default(),
            search: None,
        }
    }

    pub async fn from_env() -> anyhow::Result<Self> {
        let registry =
            std::env::var("HUB_REGISTRY_NAME").unwrap_or_else(|_| "community".to_owned());
        let database_url = std::env::var("HUB_DATABASE_URL")
            .or_else(|_| std::env::var("DATABASE_URL"))
            .ok();
        let Some(database_url) = database_url else {
            return Ok(Self::fixture());
        };

        let pool = PgPool::connect(&database_url).await?;
        run_migrations(&pool).await?;
        let search = (std::env::var("HUB_SEARCH_KIND").ok().as_deref() == Some("pgvector"))
            .then(|| PgVectorSearch::new(pool.clone()));
        Ok(Self::with_repository(
            PgHubRepository::new(pool),
            registry,
            allow_unsigned_from_env(),
            RuntimeArtifactStore::from_env()?,
            RuntimeTrustVerifier,
            RuntimeWebhookQueue::default(),
            search,
        ))
    }

    pub fn with_repository(
        repository: PgHubRepository,
        registry: impl Into<String>,
        allow_unsigned: bool,
        artifact_store: RuntimeArtifactStore,
        trust_verifier: RuntimeTrustVerifier,
        webhook_queue: RuntimeWebhookQueue,
        search: Option<PgVectorSearch>,
    ) -> Self {
        let service = HubService::new(
            repository.clone(),
            artifact_store.clone(),
            trust_verifier,
            webhook_queue,
        )
        .with_allow_unsigned(allow_unsigned);
        Self {
            registry: registry.into(),
            repository: Some(repository),
            service: Some(service),
            artifact_store,
            search,
        }
    }
}

#[derive(Clone)]
pub struct RuntimeArtifactStore {
    file: FileArtifactStore,
    s3: Option<S3ArtifactStore>,
    oci: Option<OciArtifactStore>,
}

impl RuntimeArtifactStore {
    pub fn from_env() -> Result<Self, StorageError> {
        let s3 = std::env::var("HUB_STORAGE_S3_ENDPOINT")
            .ok()
            .map(|endpoint| S3ArtifactStore::new_for_endpoint(&endpoint))
            .transpose()?;
        let oci = match (
            std::env::var("HUB_STORAGE_OCI_REGISTRY_HOST").ok(),
            std::env::var("HUB_STORAGE_OCI_BASE_URL").ok(),
        ) {
            (Some(host), Some(base_url)) => {
                Some(OciArtifactStore::new_for_registry(&host, &base_url)?)
            }
            _ => None,
        };

        Ok(Self {
            file: FileArtifactStore,
            s3,
            oci,
        })
    }

    pub async fn fetch_verified(&self, artifact: &ArtifactPointer) -> HubResult<Bytes> {
        let pointer = parse_artifact_pointer(&artifact.url).map_err(hub_storage_error)?;
        match pointer.scheme() {
            "file" => self
                .file
                .fetch_verified(&artifact.url, &artifact.digest)
                .await
                .map_err(hub_storage_error),
            "s3" => self
                .s3
                .as_ref()
                .ok_or_else(|| HubError::ArtifactVerification {
                    message: "S3 storage endpoint is not configured".to_owned(),
                })?
                .fetch_verified(&artifact.url, &artifact.digest)
                .await
                .map_err(hub_storage_error),
            "oci" => self
                .oci
                .as_ref()
                .ok_or_else(|| HubError::ArtifactVerification {
                    message: "OCI storage endpoint is not configured".to_owned(),
                })?
                .fetch_verified(&artifact.url, &artifact.digest)
                .await
                .map_err(hub_storage_error),
            scheme => Err(HubError::ArtifactVerification {
                message: format!("unsupported artifact scheme `{scheme}`"),
            }),
        }
    }
}

impl Default for RuntimeArtifactStore {
    fn default() -> Self {
        Self {
            file: FileArtifactStore,
            s3: None,
            oci: None,
        }
    }
}

#[async_trait]
impl VerifiedArtifactStore for RuntimeArtifactStore {
    async fn verify_artifact(&self, artifact: &ArtifactPointer) -> HubResult<()> {
        self.fetch_verified(artifact).await.map(|_| ())
    }
}

#[derive(Debug, Clone, Copy)]
pub struct RuntimeTrustVerifier;

#[async_trait]
impl TrustVerifier for RuntimeTrustVerifier {
    async fn verify_trust(&self, request: &PublishSkillRequest) -> HubResult<()> {
        if let (Some(signature), Some(public_key)) = (
            request.signature_ed25519.as_deref(),
            request.public_key_ed25519.as_deref(),
        ) {
            let message = trust_message(request)?;
            verify_ed25519(public_key, signature, &message).map_err(|source| {
                HubError::TrustVerification {
                    message: source.to_string(),
                }
            })?;
        } else if request.signature_ed25519.is_some() || request.public_key_ed25519.is_some() {
            return Err(HubError::TrustVerification {
                message: "Ed25519 signature and public key must be supplied together".to_owned(),
            });
        }

        if let Some(bundle) = &request.sigstore_bundle {
            verify_sigstore_bundle(&bundle.to_string()).map_err(|source| {
                HubError::TrustVerification {
                    message: source.to_string(),
                }
            })?;
        }

        Ok(())
    }
}

#[derive(Debug, Clone, Default)]
pub struct RuntimeWebhookQueue {
    events: Arc<Mutex<Vec<WebhookEvent>>>,
}

#[async_trait]
impl WebhookQueue for RuntimeWebhookQueue {
    async fn enqueue(&self, event: WebhookEvent) -> HubResult<()> {
        self.events
            .lock()
            .map_err(|_| HubError::Database {
                message: "webhook event queue lock poisoned".to_owned(),
            })?
            .push(event);
        Ok(())
    }
}

fn trust_message(request: &PublishSkillRequest) -> HubResult<Vec<u8>> {
    let mut message =
        serde_json::to_vec(&request.manifest).map_err(|source| HubError::TrustVerification {
            message: format!("failed to serialize trust message: {source}"),
        })?;
    message.push(b'|');
    message.extend_from_slice(request.artifact.digest.as_bytes());
    Ok(message)
}

fn allow_unsigned_from_env() -> bool {
    std::env::var("HUB_ALLOW_UNSIGNED")
        .ok()
        .is_some_and(|value| matches!(value.as_str(), "1" | "true" | "TRUE" | "yes" | "YES"))
}

fn hub_storage_error(source: StorageError) -> HubError {
    HubError::ArtifactVerification {
        message: source.to_string(),
    }
}
