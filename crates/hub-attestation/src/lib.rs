pub mod ed25519;
pub mod sigstore;

pub use ed25519::verify_ed25519;
pub use sigstore::verify_sigstore_bundle;

#[derive(Debug, thiserror::Error)]
pub enum AttestationError {
    #[error("invalid Ed25519 public key hex: {0}")]
    InvalidPublicKeyHex(#[source] hex::FromHexError),

    #[error("invalid Ed25519 signature hex: {0}")]
    InvalidSignatureHex(#[source] hex::FromHexError),

    #[error("invalid Ed25519 public key length: expected 32 bytes, got {actual}")]
    InvalidPublicKeyLength { actual: usize },

    #[error("invalid Ed25519 signature length: expected 64 bytes, got {actual}")]
    InvalidSignatureLength { actual: usize },

    #[error("invalid Ed25519 public key: {0}")]
    InvalidPublicKey(#[source] ed25519_dalek::SignatureError),

    #[error("invalid Ed25519 signature")]
    InvalidSignature,

    #[error("invalid Sigstore bundle JSON: {0}")]
    InvalidSigstoreJson(#[source] serde_json::Error),

    #[error("invalid Sigstore bundle: {0}")]
    InvalidSigstoreBundle(&'static str),
}
