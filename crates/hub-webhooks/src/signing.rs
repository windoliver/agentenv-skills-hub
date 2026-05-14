use hmac::{Hmac, Mac};
use sha2::Sha256;

use crate::{WebhookError, WebhookResult};

type HmacSha256 = Hmac<Sha256>;

pub fn sign_payload(secret: &str, payload: &[u8]) -> WebhookResult<String> {
    let mut mac = HmacSha256::new_from_slice(secret.as_bytes())
        .map_err(|_| WebhookError::InvalidSigningSecret)?;
    mac.update(payload);

    Ok(format!(
        "sha256={}",
        hex::encode(mac.finalize().into_bytes())
    ))
}
