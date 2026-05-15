use std::sync::Arc;

use async_trait::async_trait;

use crate::{
    auth::{can_publish, can_yank, AuthContext},
    error::{HubError, HubResult},
    model::{ArtifactPointer, PublishSkillRequest, SkillVersionRecord, Visibility},
    validation::{
        validate_artifact_url, validate_digest, validate_namespace, validate_skill_name,
        validate_skill_path, validate_version,
    },
};

#[async_trait]
pub trait HubRepository: Send + Sync {
    async fn insert_version(
        &self,
        namespace: &str,
        visibility: Visibility,
        published_by: &str,
        request: &PublishSkillRequest,
    ) -> HubResult<SkillVersionRecord>;

    async fn yank_version(
        &self,
        namespace: &str,
        name: &str,
        version: &str,
        reason: &str,
    ) -> HubResult<()>;
}

#[async_trait]
pub trait VerifiedArtifactStore: Send + Sync {
    async fn verify_artifact(&self, artifact: &ArtifactPointer) -> HubResult<()>;
}

#[async_trait]
pub trait TrustVerifier: Send + Sync {
    async fn verify_trust(&self, request: &PublishSkillRequest) -> HubResult<()>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WebhookEvent {
    pub event_type: String,
    pub namespace: String,
    pub name: String,
    pub version: String,
    pub subject: Option<String>,
}

#[async_trait]
pub trait WebhookQueue: Send + Sync {
    async fn enqueue(&self, event: WebhookEvent) -> HubResult<()>;
}

#[derive(Debug, Clone)]
pub struct HubService<R, A, T, W> {
    repo: R,
    artifact_store: A,
    trust_verifier: T,
    webhook_queue: W,
    allow_unsigned: bool,
}

impl<R, A, T, W> HubService<R, A, T, W>
where
    R: HubRepository,
    A: VerifiedArtifactStore,
    T: TrustVerifier,
    W: WebhookQueue,
{
    pub fn new(repo: R, artifact_store: A, trust_verifier: T, webhook_queue: W) -> Self {
        Self {
            repo,
            artifact_store,
            trust_verifier,
            webhook_queue,
            allow_unsigned: false,
        }
    }

    pub fn with_allow_unsigned(mut self, allow_unsigned: bool) -> Self {
        self.allow_unsigned = allow_unsigned;
        self
    }

    pub async fn publish(
        &self,
        auth: &AuthContext,
        namespace: &str,
        visibility: Visibility,
        request: &PublishSkillRequest,
    ) -> HubResult<SkillVersionRecord> {
        can_publish(auth, namespace)?;
        validate_publish_request(namespace, request)?;
        if !self.allow_unsigned && !has_signature(request) {
            return Err(HubError::UnsignedArtifactRejected);
        }

        self.artifact_store
            .verify_artifact(&request.artifact)
            .await?;
        self.trust_verifier.verify_trust(request).await?;

        let subject = authenticated_subject(auth, "publish", namespace)?;
        let record = self
            .repo
            .insert_version(namespace, visibility, subject, request)
            .await?;
        self.webhook_queue
            .enqueue(WebhookEvent {
                event_type: "skill.published".to_owned(),
                namespace: namespace.to_owned(),
                name: record.name.clone(),
                version: record.version.clone(),
                subject: Some(subject.to_owned()),
            })
            .await?;

        Ok(record)
    }

    pub async fn yank(
        &self,
        auth: &AuthContext,
        namespace: &str,
        name: &str,
        version: &str,
        reason: &str,
    ) -> HubResult<()> {
        can_yank(auth, namespace)?;
        validate_namespace(namespace)?;
        validate_skill_name(name)?;
        validate_version(version)?;

        self.repo
            .yank_version(namespace, name, version, reason)
            .await?;
        self.webhook_queue
            .enqueue(WebhookEvent {
                event_type: "skill.yanked".to_owned(),
                namespace: namespace.to_owned(),
                name: name.to_owned(),
                version: version.to_owned(),
                subject: auth.subject.clone(),
            })
            .await
    }
}

#[async_trait]
impl<T> HubRepository for Arc<T>
where
    T: HubRepository + ?Sized,
{
    async fn insert_version(
        &self,
        namespace: &str,
        visibility: Visibility,
        published_by: &str,
        request: &PublishSkillRequest,
    ) -> HubResult<SkillVersionRecord> {
        (**self)
            .insert_version(namespace, visibility, published_by, request)
            .await
    }

    async fn yank_version(
        &self,
        namespace: &str,
        name: &str,
        version: &str,
        reason: &str,
    ) -> HubResult<()> {
        (**self)
            .yank_version(namespace, name, version, reason)
            .await
    }
}

#[async_trait]
impl<T> VerifiedArtifactStore for Arc<T>
where
    T: VerifiedArtifactStore + ?Sized,
{
    async fn verify_artifact(&self, artifact: &ArtifactPointer) -> HubResult<()> {
        (**self).verify_artifact(artifact).await
    }
}

#[async_trait]
impl<T> TrustVerifier for Arc<T>
where
    T: TrustVerifier + ?Sized,
{
    async fn verify_trust(&self, request: &PublishSkillRequest) -> HubResult<()> {
        (**self).verify_trust(request).await
    }
}

#[async_trait]
impl<T> WebhookQueue for Arc<T>
where
    T: WebhookQueue + ?Sized,
{
    async fn enqueue(&self, event: WebhookEvent) -> HubResult<()> {
        (**self).enqueue(event).await
    }
}

fn validate_publish_request(namespace: &str, request: &PublishSkillRequest) -> HubResult<()> {
    validate_namespace(namespace)?;
    validate_skill_name(&request.manifest.name)?;
    validate_version(&request.manifest.version)?;
    validate_digest(&request.artifact.digest)?;
    if let Some(bundle_digest) = request.bundle_digest.as_deref() {
        validate_digest(bundle_digest)?;
    }
    validate_artifact_url(&request.artifact.url)?;
    validate_skill_path(&request.manifest.entry)?;
    for file in &request.manifest.files {
        validate_skill_path(file)?;
    }
    Ok(())
}

fn has_signature(request: &PublishSkillRequest) -> bool {
    (request.signature_ed25519.is_some() && request.public_key_ed25519.is_some())
        || request.sigstore_bundle.is_some()
}

fn authenticated_subject<'a>(
    auth: &'a AuthContext,
    action: &str,
    namespace: &str,
) -> HubResult<&'a str> {
    auth.subject
        .as_deref()
        .ok_or_else(|| HubError::PermissionDenied {
            action: action.to_owned(),
            namespace: namespace.to_owned(),
        })
}
