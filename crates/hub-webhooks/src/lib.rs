pub mod model;
pub mod render;
pub mod signing;
pub mod worker;

pub use model::{WebhookDelivery, WebhookEvent, WebhookKind};
pub use render::render_payload;
pub use signing::sign_payload;
pub use worker::retry_delay;

pub type WebhookResult<T> = Result<T, WebhookError>;

#[derive(Debug, thiserror::Error)]
pub enum WebhookError {
    #[error("failed to render webhook payload: {0}")]
    Render(#[from] serde_json::Error),
    #[error("invalid webhook signing secret")]
    InvalidSigningSecret,
}
