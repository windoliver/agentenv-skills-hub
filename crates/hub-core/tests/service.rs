use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use hub_core::{
    auth::AuthContext,
    error::{HubError, HubResult},
    model::{
        ArtifactPointer, NamespaceRole, PublishSkillRequest, SkillManifest, SkillVersionRecord,
        Visibility,
    },
    service::{
        HubRepository, HubService, TrustVerifier, VerifiedArtifactStore, WebhookEvent, WebhookQueue,
    },
};
use time::OffsetDateTime;
use uuid::Uuid;

const DIGEST: &str = "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

#[derive(Debug, Default)]
struct FakeRepository {
    insert_calls: Mutex<Vec<InsertCall>>,
}

#[derive(Debug, Clone)]
struct InsertCall {
    namespace: String,
    visibility: Visibility,
    published_by: String,
    request: PublishSkillRequest,
}

#[async_trait]
impl HubRepository for FakeRepository {
    async fn insert_version(
        &self,
        namespace: &str,
        visibility: Visibility,
        published_by: &str,
        request: &PublishSkillRequest,
    ) -> HubResult<SkillVersionRecord> {
        self.insert_calls.lock().unwrap().push(InsertCall {
            namespace: namespace.to_owned(),
            visibility,
            published_by: published_by.to_owned(),
            request: request.clone(),
        });

        Ok(record(namespace, request))
    }

    async fn yank_version(
        &self,
        _namespace: &str,
        _name: &str,
        _version: &str,
        _reason: &str,
    ) -> HubResult<()> {
        Ok(())
    }
}

#[derive(Debug, Default)]
struct FakeArtifactStore {
    verified: Mutex<Vec<ArtifactPointer>>,
}

#[async_trait]
impl VerifiedArtifactStore for FakeArtifactStore {
    async fn verify_artifact(&self, artifact: &ArtifactPointer) -> HubResult<()> {
        self.verified.lock().unwrap().push(artifact.clone());
        Ok(())
    }
}

#[derive(Debug, Default)]
struct FakeTrustVerifier {
    verified: Mutex<Vec<PublishSkillRequest>>,
}

#[async_trait]
impl TrustVerifier for FakeTrustVerifier {
    async fn verify_trust(&self, request: &PublishSkillRequest) -> HubResult<()> {
        self.verified.lock().unwrap().push(request.clone());
        Ok(())
    }
}

#[derive(Debug, Default)]
struct FakeWebhookQueue {
    events: Mutex<Vec<WebhookEvent>>,
}

#[async_trait]
impl WebhookQueue for FakeWebhookQueue {
    async fn enqueue(&self, event: WebhookEvent) -> HubResult<()> {
        self.events.lock().unwrap().push(event);
        Ok(())
    }
}

#[tokio::test]
async fn publish_validates_artifact_and_trust_inserts_version_and_enqueues_webhook() {
    let repo = Arc::new(FakeRepository::default());
    let artifacts = Arc::new(FakeArtifactStore::default());
    let trust = Arc::new(FakeTrustVerifier::default());
    let webhooks = Arc::new(FakeWebhookQueue::default());
    let service = HubService::new(
        Arc::clone(&repo),
        Arc::clone(&artifacts),
        Arc::clone(&trust),
        Arc::clone(&webhooks),
    );
    let publisher = AuthContext::new(
        "alice",
        ["skills:publish"],
        [("community", NamespaceRole::Publisher)],
    );
    let request = signed_request();

    let record = service
        .publish(&publisher, "community", Visibility::Public, &request)
        .await
        .unwrap();

    assert_eq!(record.namespace, "community");
    assert_eq!(record.name, "code-review");
    assert_eq!(record.version, "1.2.3");

    let artifact_calls = artifacts.verified.lock().unwrap();
    assert_eq!(
        artifact_calls.as_slice(),
        std::slice::from_ref(&request.artifact)
    );

    let trust_calls = trust.verified.lock().unwrap();
    assert_eq!(trust_calls.as_slice(), std::slice::from_ref(&request));

    let insert_calls = repo.insert_calls.lock().unwrap();
    assert_eq!(insert_calls.len(), 1);
    assert_eq!(insert_calls[0].namespace, "community");
    assert_eq!(insert_calls[0].visibility, Visibility::Public);
    assert_eq!(insert_calls[0].published_by, "alice");
    assert_eq!(insert_calls[0].request, request);

    let events = webhooks.events.lock().unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].event_type, "skill.published");
    assert_eq!(events[0].namespace, "community");
    assert_eq!(events[0].name, "code-review");
    assert_eq!(events[0].version, "1.2.3");
    assert_eq!(events[0].subject.as_deref(), Some("alice"));
}

#[tokio::test]
async fn publish_rejects_unsigned_artifacts_by_default() {
    let repo = Arc::new(FakeRepository::default());
    let artifacts = Arc::new(FakeArtifactStore::default());
    let trust = Arc::new(FakeTrustVerifier::default());
    let webhooks = Arc::new(FakeWebhookQueue::default());
    let service = HubService::new(
        Arc::clone(&repo),
        Arc::clone(&artifacts),
        Arc::clone(&trust),
        Arc::clone(&webhooks),
    );
    let publisher = AuthContext::new(
        "alice",
        ["skills:publish"],
        [("community", NamespaceRole::Publisher)],
    );
    let mut request = signed_request();
    request.signature_ed25519 = None;
    request.public_key_ed25519 = None;
    request.sigstore_bundle = None;

    let error = service
        .publish(&publisher, "community", Visibility::Public, &request)
        .await
        .unwrap_err();

    assert!(matches!(error, HubError::UnsignedArtifactRejected));
    assert!(repo.insert_calls.lock().unwrap().is_empty());
    assert!(artifacts.verified.lock().unwrap().is_empty());
    assert!(trust.verified.lock().unwrap().is_empty());
    assert!(webhooks.events.lock().unwrap().is_empty());
}

fn signed_request() -> PublishSkillRequest {
    PublishSkillRequest {
        manifest: SkillManifest {
            name: "code-review".to_owned(),
            version: "1.2.3".to_owned(),
            description: Some("Review code changes".to_owned()),
            entry: "SKILL.md".to_owned(),
            files: vec!["SKILL.md".to_owned(), "references/checklist.md".to_owned()],
        },
        artifact: ArtifactPointer {
            url: "file:///tmp/code-review-1.2.3.tar.zst".to_owned(),
            media_type: "application/vnd.agentenv.skill.v1+tar".to_owned(),
            digest: DIGEST.to_owned(),
        },
        signature_ed25519: Some("aa".repeat(64)),
        public_key_ed25519: Some("bb".repeat(32)),
        sigstore_bundle: None,
    }
}

fn record(namespace: &str, request: &PublishSkillRequest) -> SkillVersionRecord {
    SkillVersionRecord {
        id: Uuid::new_v4(),
        namespace: namespace.to_owned(),
        name: request.manifest.name.clone(),
        version: request.manifest.version.clone(),
        description: request.manifest.description.clone(),
        digest: request.artifact.digest.clone(),
        artifact_url: request.artifact.url.clone(),
        artifact_media_type: request.artifact.media_type.clone(),
        signature_ed25519: request.signature_ed25519.clone(),
        public_key_ed25519: request.public_key_ed25519.clone(),
        yanked_at: None,
        yank_reason: None,
        created_at: OffsetDateTime::now_utc(),
    }
}
