//! client-runtime: Blob crypto and send order. Each image's WebP is encrypted
//! under a fresh 256-bit key with XChaCha20-Poly1305, and the blob id is the
//! additional authenticated data, so a blob served under another id fails to
//! decrypt. The key travels only inside the MLS message.

use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};
use hmac::{Hmac, Mac};
use protocol::blob_store::{
    BLOB_ID_LEN, BLOB_KEY_LEN, BLOB_NONCE_CONTEXT, BLOB_NONCE_LEN, BLOB_TAG_LEN,
};
use sha2::Sha256;

/// A blob id.
pub type BlobId = [u8; BLOB_ID_LEN];
/// A blob key.
pub type BlobKey = [u8; BLOB_KEY_LEN];

/// Why a blob could not be made or opened.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum CryptoError {
    /// No randomness from the operating system.
    #[error("no randomness available")]
    NoRandomness,
    /// The blob is too short, or does not authenticate under this key and id:
    /// damaged, or not the blob this message names.
    #[error("the blob does not decrypt")]
    Invalid,
}

/// A fresh random blob id and key, never reused (no dedup).
pub fn fresh() -> Result<(BlobId, BlobKey), CryptoError> {
    let mut id = [0u8; BLOB_ID_LEN];
    let mut key = [0u8; BLOB_KEY_LEN];
    getrandom::fill(&mut id).map_err(|_| CryptoError::NoRandomness)?;
    getrandom::fill(&mut key).map_err(|_| CryptoError::NoRandomness)?;
    Ok((id, key))
}

/// The nonce for `key` and `id`: derived, not drawn, so encrypting the same
/// image again gives the same bytes (an idempotent upload).
fn nonce(key: &BlobKey, id: &BlobId) -> Result<[u8; BLOB_NONCE_LEN], CryptoError> {
    let mut mac = <Hmac<Sha256> as Mac>::new_from_slice(key).map_err(|_| CryptoError::Invalid)?;
    mac.update(BLOB_NONCE_CONTEXT);
    mac.update(id);
    let tag = mac.finalize().into_bytes();
    let mut nonce = [0u8; BLOB_NONCE_LEN];
    nonce.copy_from_slice(tag.get(..BLOB_NONCE_LEN).ok_or(CryptoError::Invalid)?);
    Ok(nonce)
}

/// The blob for `plaintext`: the nonce, then the ciphertext and its tag.
pub fn encrypt(key: &BlobKey, id: &BlobId, plaintext: &[u8]) -> Result<Vec<u8>, CryptoError> {
    let nonce = nonce(key, id)?;
    let sealed = XChaCha20Poly1305::new(key.into())
        .encrypt(
            XNonce::from_slice(&nonce),
            Payload {
                msg: plaintext,
                aad: id,
            },
        )
        .map_err(|_| CryptoError::Invalid)?;
    let mut blob = Vec::with_capacity(BLOB_NONCE_LEN + sealed.len());
    blob.extend_from_slice(&nonce);
    blob.extend_from_slice(&sealed);
    Ok(blob)
}

/// The plaintext of `blob`, if it authenticates under `key` and `id`.
pub fn decrypt(key: &BlobKey, id: &BlobId, blob: &[u8]) -> Result<Vec<u8>, CryptoError> {
    if blob.len() < BLOB_NONCE_LEN + BLOB_TAG_LEN {
        return Err(CryptoError::Invalid);
    }
    let (nonce, sealed) = blob.split_at(BLOB_NONCE_LEN);
    XChaCha20Poly1305::new(key.into())
        .decrypt(
            XNonce::from_slice(nonce),
            Payload {
                msg: sealed,
                aad: id,
            },
        )
        .map_err(|_| CryptoError::Invalid)
}
