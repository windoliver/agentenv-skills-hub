use ed25519_dalek::{Signer, SigningKey};
use hub_attestation::{verify_ed25519, verify_sigstore_bundle};

#[test]
fn valid_ed25519_signature_over_manifest_digest_message_verifies() {
    let signing_key = SigningKey::from_bytes(&[7; 32]);
    let message =
        b"manifest:sha256:2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824";
    let signature = signing_key.sign(message);

    let public_key_hex = hex::encode(signing_key.verifying_key().to_bytes());
    let signature_hex = hex::encode(signature.to_bytes());

    verify_ed25519(&public_key_hex, &signature_hex, message).expect("signature should verify");
}

#[test]
fn invalid_ed25519_signature_returns_invalid_error() {
    let signing_key = SigningKey::from_bytes(&[7; 32]);
    let other_key = SigningKey::from_bytes(&[8; 32]);
    let message =
        b"manifest:sha256:2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824";
    let signature = other_key.sign(message);

    let public_key_hex = hex::encode(signing_key.verifying_key().to_bytes());
    let signature_hex = hex::encode(signature.to_bytes());

    let err = verify_ed25519(&public_key_hex, &signature_hex, message)
        .expect_err("signature should be invalid");

    assert!(err.to_string().contains("invalid"));
}

#[test]
fn sigstore_bundle_shape_accepts_minimal_message_signature_bundle() {
    let bundle = r#"{
        "mediaType": "application/vnd.dev.sigstore.bundle+json;version=0.3",
        "verificationMaterial": {"certificate": "placeholder"},
        "messageSignature": {"messageDigest": {"algorithm": "SHA2_256", "digest": "abc"}}
    }"#;

    verify_sigstore_bundle(bundle).expect("bundle shape should be accepted");
}

#[test]
fn sigstore_bundle_shape_rejects_malformed_json() {
    let err = verify_sigstore_bundle("{").expect_err("malformed JSON should be rejected");

    assert!(err.to_string().contains("invalid"));
}
