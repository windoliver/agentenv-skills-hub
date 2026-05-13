use ed25519_dalek::{Signature, Verifier, VerifyingKey};

use crate::AttestationError;

pub fn verify_ed25519(
    public_key_hex: &str,
    signature_hex: &str,
    message: &[u8],
) -> Result<(), AttestationError> {
    let public_key_bytes =
        hex::decode(public_key_hex).map_err(AttestationError::InvalidPublicKeyHex)?;
    let public_key: [u8; 32] = public_key_bytes.as_slice().try_into().map_err(|_| {
        AttestationError::InvalidPublicKeyLength {
            actual: public_key_bytes.len(),
        }
    })?;
    let verifying_key =
        VerifyingKey::from_bytes(&public_key).map_err(AttestationError::InvalidPublicKey)?;

    let signature_bytes =
        hex::decode(signature_hex).map_err(AttestationError::InvalidSignatureHex)?;
    let signature: [u8; 64] = signature_bytes.as_slice().try_into().map_err(|_| {
        AttestationError::InvalidSignatureLength {
            actual: signature_bytes.len(),
        }
    })?;
    let signature = Signature::from_bytes(&signature);

    verifying_key
        .verify(message, &signature)
        .map_err(|_| AttestationError::InvalidSignature)
}
