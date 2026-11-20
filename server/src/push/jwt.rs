//! The two JWTs the server signs (push design D2, D3): RS256 for Google's OAuth
//! JWT bearer grant and ES256 for APNs provider tokens. Signed with ring.

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use ring::rand::SystemRandom;
use ring::signature::{EcdsaKeyPair, RSA_PKCS1_SHA256, RsaKeyPair};

/// Signing failed (ring reports no detail).
#[derive(Debug, thiserror::Error)]
#[error("signing the push JWT failed")]
pub struct SignError;

fn signing_input(header: &serde_json::Value, claims: &serde_json::Value) -> String {
    format!(
        "{}.{}",
        URL_SAFE_NO_PAD.encode(header.to_string()),
        URL_SAFE_NO_PAD.encode(claims.to_string())
    )
}

/// `header.claims.signature` with RS256.
pub fn rs256(
    key: &RsaKeyPair,
    header: serde_json::Value,
    claims: serde_json::Value,
) -> Result<String, SignError> {
    let input = signing_input(&header, &claims);
    let mut signature = vec![0; key.public().modulus_len()];
    key.sign(
        &RSA_PKCS1_SHA256,
        &SystemRandom::new(),
        input.as_bytes(),
        &mut signature,
    )
    .map_err(|_| SignError)?;
    Ok(format!("{input}.{}", URL_SAFE_NO_PAD.encode(signature)))
}

/// `header.claims.signature` with ES256 (fixed-size r || s, as JWS wants).
pub fn es256(
    key: &EcdsaKeyPair,
    header: serde_json::Value,
    claims: serde_json::Value,
) -> Result<String, SignError> {
    let input = signing_input(&header, &claims);
    let signature = key
        .sign(&SystemRandom::new(), input.as_bytes())
        .map_err(|_| SignError)?;
    Ok(format!(
        "{input}.{}",
        URL_SAFE_NO_PAD.encode(signature.as_ref())
    ))
}

/// Seconds since the Unix epoch (`iat`).
pub fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}
