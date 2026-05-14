use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use hub_core::error::HubError;

#[derive(Debug)]
pub struct ApiError {
    pub status: StatusCode,
    pub message: String,
}

impl ApiError {
    pub fn bad_request(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            message: message.into(),
        }
    }

    pub fn unavailable(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::SERVICE_UNAVAILABLE,
            message: message.into(),
        }
    }

    pub fn internal(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            message: message.into(),
        }
    }
}

impl From<HubError> for ApiError {
    fn from(error: HubError) -> Self {
        let status = match error {
            HubError::PermissionDenied { .. } => StatusCode::FORBIDDEN,
            HubError::SkillVersionNotFound { .. } => StatusCode::NOT_FOUND,
            HubError::Database { .. } => StatusCode::INTERNAL_SERVER_ERROR,
            HubError::VersionDigestConflict { .. } => StatusCode::CONFLICT,
            HubError::InvalidNamespace { .. }
            | HubError::InvalidSkillName { .. }
            | HubError::InvalidVersion { .. }
            | HubError::InvalidDigest { .. }
            | HubError::UnsafeSkillPath { .. }
            | HubError::InvalidArtifactUrl { .. }
            | HubError::UnsignedArtifactRejected
            | HubError::ArtifactVerification { .. }
            | HubError::TrustVerification { .. } => StatusCode::BAD_REQUEST,
        };
        Self {
            status,
            message: error.to_string(),
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (
            self.status,
            Json(serde_json::json!({ "error": self.message })),
        )
            .into_response()
    }
}
