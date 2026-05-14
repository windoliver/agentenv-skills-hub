use hub_webhooks::{render_payload, sign_payload, WebhookEvent, WebhookKind};

fn webhook_event() -> WebhookEvent {
    WebhookEvent {
        event_type: "skill.published".to_string(),
        namespace: "community".to_string(),
        skill: "code-review".to_string(),
        version: Some("1.0.0".to_string()),
        actor: Some("alice".to_string()),
    }
}

#[test]
fn renders_payloads_with_skill_name() {
    let event = webhook_event();

    for kind in [
        WebhookKind::Generic,
        WebhookKind::Slack,
        WebhookKind::Discord,
        WebhookKind::Matrix,
    ] {
        let payload = render_payload(kind, &event).expect("payload renders");

        assert!(
            payload.contains("code-review"),
            "{kind:?} payload should include the skill name: {payload}"
        );
    }
}

#[test]
fn signs_payload_with_hmac_sha256_prefix() {
    let signature = sign_payload("secret", br#"{"skill":"code-review"}"#).expect("payload signs");

    assert!(signature.starts_with("sha256="));
}
