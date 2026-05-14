use time::OffsetDateTime;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum WebhookKind {
    Generic,
    Slack,
    Discord,
    Matrix,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
pub struct WebhookEvent {
    pub event_type: String,
    pub namespace: String,
    pub skill: String,
    pub version: Option<String>,
    pub actor: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
pub struct WebhookDelivery {
    pub id: Uuid,
    pub kind: WebhookKind,
    pub event: WebhookEvent,
    pub payload: String,
    pub attempts: u32,
    pub next_attempt_at: Option<OffsetDateTime>,
    pub last_error: Option<String>,
}
