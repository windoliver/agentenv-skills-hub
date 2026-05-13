use serde_json::Value;

use crate::AttestationError;

const SIGSTORE_BUNDLE_MEDIA_TYPE: &str = "application/vnd.dev.sigstore.bundle+json";

pub fn verify_sigstore_bundle(bundle_json: &str) -> Result<(), AttestationError> {
    let bundle: Value =
        serde_json::from_str(bundle_json).map_err(AttestationError::InvalidSigstoreJson)?;
    let bundle = bundle
        .as_object()
        .ok_or(AttestationError::InvalidSigstoreBundle(
            "bundle must be a JSON object",
        ))?;

    let media_type = bundle.get("mediaType").and_then(Value::as_str).ok_or(
        AttestationError::InvalidSigstoreBundle("mediaType is required"),
    )?;
    if !media_type.contains(SIGSTORE_BUNDLE_MEDIA_TYPE) {
        return Err(AttestationError::InvalidSigstoreBundle(
            "mediaType must be a Sigstore bundle",
        ));
    }

    if !bundle.contains_key("verificationMaterial") {
        return Err(AttestationError::InvalidSigstoreBundle(
            "verificationMaterial is required",
        ));
    }

    if !bundle.contains_key("messageSignature") && !bundle.contains_key("dsseEnvelope") {
        return Err(AttestationError::InvalidSigstoreBundle(
            "messageSignature or dsseEnvelope is required",
        ));
    }

    Ok(())
}
