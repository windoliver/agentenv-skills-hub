use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Visibility {
    Public,
    Private,
    Unlisted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NamespaceRole {
    Reader,
    Publisher,
    Admin,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkillManifest {
    pub name: String,
    pub version: String,
    pub description: Option<String>,
    pub entry: String,
    pub files: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArtifactPointer {
    pub url: String,
    pub media_type: String,
    pub digest: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublishSkillRequest {
    pub manifest: SkillManifest,
    pub artifact: ArtifactPointer,
    pub signature_ed25519: Option<String>,
    pub public_key_ed25519: Option<String>,
    pub sigstore_bundle: Option<serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkillVersionRecord {
    pub id: Uuid,
    pub namespace: String,
    pub name: String,
    pub version: String,
    pub description: Option<String>,
    pub digest: String,
    pub artifact_url: String,
    pub artifact_media_type: String,
    pub signature_ed25519: Option<String>,
    pub public_key_ed25519: Option<String>,
    pub yanked_at: Option<OffsetDateTime>,
    pub yank_reason: Option<String>,
    pub created_at: OffsetDateTime,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompatibilityIndex {
    pub skills: Vec<CompatibilitySkillHit>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompatibilitySkillHit {
    pub name: String,
    pub version: String,
    pub description: Option<String>,
    pub registry: String,
    pub digest: Option<String>,
    pub signature_ed25519: Option<String>,
    pub public_key_ed25519: Option<String>,
}
