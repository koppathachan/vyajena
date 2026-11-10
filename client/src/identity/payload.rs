//! The payloads a UI shows as a QR code or text and the other side scans or
//! pastes (device-identity: Invites; Device linking). The core only produces
//! and consumes bytes; [`to_text`] and [`from_text`] give the text form the Pi
//! prints (base64url without padding).
//!
//! All three are encodings of `vyajena.v1` messages, so no message exists that
//! the contract does not define (wire-transport: Protobuf contract is the
//! source of truth):
//!
//! - invite: `InvitePayload` with the relay address, the TLS pin, the invite
//!   code and the user id (`Admin.CreateInvite`, or the Pi's bootstrap log);
//! - link request (shown by the new device): `DeviceCertificate` holding only
//!   the device id, the auth public key and the MLS signature public key;
//! - link approval (shown by the identity device after `Devices.LinkDevice`):
//!   `InvitePayload` with the relay address, the TLS pin and the user id and
//!   no invite code, so the new device knows where to connect and as whom.

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use prost::Message as _;
use protocol::device_identity::{DeviceId, ED25519_PUBLIC_KEY_LEN, device_id};

use crate::events::UserId;
use crate::pb;

/// Why a payload was refused.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PayloadError {
    /// The text is not base64url.
    #[error("the payload text is not base64url")]
    NotBase64,
    /// The bytes do not decode as the expected message.
    #[error("the payload does not decode")]
    Undecodable,
    /// A field is missing or malformed.
    #[error("the payload is invalid: {0}")]
    Invalid(&'static str),
}

/// A payload in text form: base64url without padding, as the Pi prints its
/// bootstrap invite.
pub fn to_text(bytes: &[u8]) -> String {
    URL_SAFE_NO_PAD.encode(bytes)
}

/// The bytes of a payload in text form. Surrounding whitespace is ignored.
pub fn from_text(text: &str) -> Result<Vec<u8>, PayloadError> {
    URL_SAFE_NO_PAD
        .decode(text.trim())
        .map_err(|_| PayloadError::NotBase64)
}

fn pin(bytes: &[u8]) -> Result<[u8; 32], PayloadError> {
    bytes
        .try_into()
        .map_err(|_| PayloadError::Invalid("the TLS pin is not 32 bytes"))
}

fn user(user: Option<&pb::UserId>) -> Result<UserId, PayloadError> {
    let user = user.ok_or(PayloadError::Invalid("no user id"))?;
    if user.username.is_empty() || user.server_id.is_empty() {
        return Err(PayloadError::Invalid("the user id is incomplete"));
    }
    Ok(UserId::from_pb(user))
}

/// An invite: where the Pi is, how to recognise it, the code and the user it
/// creates.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Invite {
    /// `host:port` of the relay.
    pub relay_address: String,
    /// SHA-256 of the Pi's TLS SubjectPublicKeyInfo.
    pub tls_pin: [u8; 32],
    /// The single-use invite code.
    pub code: String,
    /// The user id the invite creates.
    pub user: UserId,
}

impl Invite {
    /// Reads an encoded `InvitePayload`.
    pub fn decode(bytes: &[u8]) -> Result<Self, PayloadError> {
        let payload = pb::InvitePayload::decode(bytes).map_err(|_| PayloadError::Undecodable)?;
        if payload.invite_code.is_empty() {
            return Err(PayloadError::Invalid(
                "no invite code (is this a link approval?)",
            ));
        }
        if payload.relay_address.is_empty() {
            return Err(PayloadError::Invalid("no relay address"));
        }
        Ok(Self {
            relay_address: payload.relay_address,
            tls_pin: pin(&payload.tls_spki_sha256)?,
            code: payload.invite_code,
            user: user(payload.user_id.as_ref())?,
        })
    }

    /// The encoded `InvitePayload`.
    pub fn encode(&self) -> Vec<u8> {
        pb::InvitePayload {
            relay_address: self.relay_address.clone(),
            tls_spki_sha256: self.tls_pin.to_vec(),
            invite_code: self.code.clone(),
            user_id: Some(self.user.to_pb()),
        }
        .encode_to_vec()
    }
}

/// What a new device shows so the identity device can certify it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinkRequest {
    /// The new device's auth public key.
    pub auth_public_key: [u8; ED25519_PUBLIC_KEY_LEN],
    /// The new device's MLS signature public key.
    pub mls_signature_public_key: [u8; ED25519_PUBLIC_KEY_LEN],
}

impl LinkRequest {
    /// The new device's id.
    pub fn device_id(&self) -> DeviceId {
        device_id(&self.auth_public_key)
    }

    /// Reads an encoded link request.
    pub fn decode(bytes: &[u8]) -> Result<Self, PayloadError> {
        let cert = pb::DeviceCertificate::decode(bytes).map_err(|_| PayloadError::Undecodable)?;
        let auth: [u8; ED25519_PUBLIC_KEY_LEN] = cert
            .auth_public_key
            .as_slice()
            .try_into()
            .map_err(|_| PayloadError::Invalid("the auth key is not 32 bytes"))?;
        let mls: [u8; ED25519_PUBLIC_KEY_LEN] = cert
            .mls_signature_public_key
            .as_slice()
            .try_into()
            .map_err(|_| PayloadError::Invalid("the MLS key is not 32 bytes"))?;
        if let Some(device) = &cert.device
            && device.device_id != device_id(&auth)
        {
            return Err(PayloadError::Invalid(
                "the device id is not derived from the auth key",
            ));
        }
        Ok(Self {
            auth_public_key: auth,
            mls_signature_public_key: mls,
        })
    }

    /// The encoded link request: a `DeviceCertificate` with only the device
    /// id and the two public keys.
    pub fn encode(&self) -> Vec<u8> {
        pb::DeviceCertificate {
            device: Some(pb::DeviceRef {
                user_id: None,
                device_id: self.device_id().to_vec(),
            }),
            auth_public_key: self.auth_public_key.to_vec(),
            mls_signature_public_key: self.mls_signature_public_key.to_vec(),
            created_at: None,
        }
        .encode_to_vec()
    }
}

/// What the identity device shows once it certified a new device.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinkApproval {
    /// `host:port` of the relay.
    pub relay_address: String,
    /// SHA-256 of the Pi's TLS SubjectPublicKeyInfo.
    pub tls_pin: [u8; 32],
    /// The user the new device now belongs to.
    pub user: UserId,
}

impl LinkApproval {
    /// Reads an encoded link approval.
    pub fn decode(bytes: &[u8]) -> Result<Self, PayloadError> {
        let payload = pb::InvitePayload::decode(bytes).map_err(|_| PayloadError::Undecodable)?;
        if !payload.invite_code.is_empty() {
            return Err(PayloadError::Invalid(
                "this is an invite, not a link approval",
            ));
        }
        if payload.relay_address.is_empty() {
            return Err(PayloadError::Invalid("no relay address"));
        }
        Ok(Self {
            relay_address: payload.relay_address,
            tls_pin: pin(&payload.tls_spki_sha256)?,
            user: user(payload.user_id.as_ref())?,
        })
    }

    /// The encoded approval: an `InvitePayload` without an invite code.
    pub fn encode(&self) -> Vec<u8> {
        pb::InvitePayload {
            relay_address: self.relay_address.clone(),
            tls_spki_sha256: self.tls_pin.to_vec(),
            invite_code: String::new(),
            user_id: Some(self.user.to_pb()),
        }
        .encode_to_vec()
    }
}
