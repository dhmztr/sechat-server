use chrono::Utc;
use sha2::{Digest, Sha256};

use crate::*;

pub fn identity_hash(pub_key: &[u8; 32], verif: &[u8; 32]) -> [u8; 32] {
    Sha256::new()
        .chain_update(pub_key)
        .chain_update(verif)
        .finalize()
        .into()
}

pub fn verify_message(
    pubkey: &[u8; 32],
    verif: &[u8; 32],
    signature: Vec<u8>,
    timestamp: i64,
) -> Result<(), ServerError> {
    let now = Utc::now().timestamp();
    if (now - timestamp).abs() > 30 {
        return Err(ServerError::TimestampOutOfRange);
    }
    let bytes_to_verify = [
        pubkey.as_slice(),
        verif.as_slice(),
        &timestamp.to_le_bytes(),
    ]
    .concat();

    let verifying_key = match VerifyingKey::from_bytes(&verif) {
        Ok(key) => key,
        Err(_) => return Err(ServerError::InvalidPublicKey),
    };
    let sign_bytes: [u8; 64] = signature
        .try_into()
        .map_err(|_| ServerError::InvalidSignature)?;
    let sig = Signature::from_bytes(&sign_bytes);
    verifying_key
        .verify(&bytes_to_verify, &sig)
        .map_err(|_| ServerError::InvalidSignature)
}
