use std::path::PathBuf;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum HubError {
    #[error("invalid namespace `{value}`")]
    InvalidNamespace { value: String },
    #[error("invalid skill name `{value}`")]
    InvalidSkillName { value: String },
    #[error("invalid version `{value}`: {source}")]
    InvalidVersion {
        value: String,
        #[source]
        source: semver::Error,
    },
    #[error("invalid digest `{value}`")]
    InvalidDigest { value: String },
    #[error("unsafe skill path `{path}`")]
    UnsafeSkillPath { path: PathBuf },
    #[error("invalid artifact URL `{value}`: {message}")]
    InvalidArtifactUrl { value: String, message: String },
    #[error("permission denied for `{action}` on namespace `{namespace}`")]
    PermissionDenied { action: String, namespace: String },
    #[error("unsigned skill artifacts are not allowed")]
    UnsignedArtifactRejected,
    #[error("skill `{namespace}/{name}` version `{version}` already exists with another digest")]
    VersionDigestConflict {
        namespace: String,
        name: String,
        version: String,
    },
    #[error("skill `{namespace}/{name}` version `{version}` was not found")]
    SkillVersionNotFound {
        namespace: String,
        name: String,
        version: String,
    },
    #[error("database error: {message}")]
    Database { message: String },
}

pub type HubResult<T> = Result<T, HubError>;
