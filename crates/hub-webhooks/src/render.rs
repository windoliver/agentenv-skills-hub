use serde_json::json;

use crate::{WebhookEvent, WebhookKind, WebhookResult};

pub fn render_payload(kind: WebhookKind, event: &WebhookEvent) -> WebhookResult<String> {
    let payload = match kind {
        WebhookKind::Generic => json!({
            "event": event.event_type,
            "namespace": event.namespace,
            "skill": event.skill,
            "version": event.version,
            "actor": event.actor,
        }),
        WebhookKind::Slack => json!({
            "text": event_summary(event),
        }),
        WebhookKind::Discord => json!({
            "content": event_summary(event),
        }),
        WebhookKind::Matrix => json!({
            "msgtype": "m.notice",
            "body": event_summary(event),
        }),
    };

    serde_json::to_string(&payload).map_err(Into::into)
}

fn event_summary(event: &WebhookEvent) -> String {
    match (&event.version, &event.actor) {
        (Some(version), Some(actor)) => {
            format!(
                "{}: {}/{}@{} by {}",
                event.event_type, event.namespace, event.skill, version, actor
            )
        }
        (Some(version), None) => {
            format!(
                "{}: {}/{}@{}",
                event.event_type, event.namespace, event.skill, version
            )
        }
        (None, Some(actor)) => {
            format!(
                "{}: {}/{} by {}",
                event.event_type, event.namespace, event.skill, actor
            )
        }
        (None, None) => format!("{}: {}/{}", event.event_type, event.namespace, event.skill),
    }
}
