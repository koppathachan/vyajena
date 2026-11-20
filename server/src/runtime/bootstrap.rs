//! device-identity: Bootstrap admin. On a start with no users and no usable
//! bootstrap invite, the server creates one invite for the configured admin user id
//! and prints its payload to the log once. The user created from it is the
//! admin (the admin is whoever `admin_username` names).

use std::time::SystemTime;

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use prost::Message as _;

use crate::admin::invites::{self, Invite};
use crate::pb;
use crate::store::{Store, StoreError};

use super::config::Config;
use super::server_keys::SpkiPin;

/// What the bootstrap step found or did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Bootstrap {
    /// No users and no usable bootstrap invite: one was created now.
    Created(BootstrapInvite),
    /// No users yet, but a bootstrap invite created earlier is still usable. It
    /// was printed when it was created and is not printed again.
    Pending {
        /// When the pending invite expires; after that the next start creates
        /// a new one.
        expires_at: SystemTime,
    },
    /// Users exist; there is nothing to bootstrap.
    NotNeeded,
}

/// A newly created bootstrap invite and the payload to hand to the admin.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BootstrapInvite {
    /// The stored invite.
    pub invite: Invite,
    /// The invite payload: relay address, TLS pin, code and user id.
    pub payload: pb::InvitePayload,
}

impl BootstrapInvite {
    /// The payload in text form, as printed: the encoded `InvitePayload` in
    /// base64url without padding.
    pub fn payload_text(&self) -> String {
        encode_payload(&self.payload)
    }
}

/// An invite payload as text: the encoded `InvitePayload`, base64url without
/// padding.
pub fn encode_payload(payload: &pb::InvitePayload) -> String {
    URL_SAFE_NO_PAD.encode(payload.encode_to_vec())
}

/// Creates the bootstrap invite if the database has no users and no usable
/// bootstrap invite, in one transaction.
pub async fn ensure_bootstrap_invite(
    store: &Store,
    config: &Config,
    tls_spki_sha256: SpkiPin,
    now: SystemTime,
) -> Result<Bootstrap, StoreError> {
    let admin = config.admin_username.clone();
    let server_id = config.server_id.clone();
    let relay_address = config.public_address.clone();
    store
        .call(move |conn| {
            let tx = conn.transaction()?;
            let users: i64 = tx.query_row("SELECT count(*) FROM users", [], |row| row.get(0))?;
            if users > 0 {
                return Ok(Bootstrap::NotNeeded);
            }
            if let Some(pending) = invites::usable_bootstrap(&tx, now)? {
                return Ok(Bootstrap::Pending {
                    expires_at: pending.expires_at,
                });
            }
            let invite = invites::create(&tx, &admin, now, true)?;
            tx.commit()?;
            let payload = pb::InvitePayload {
                relay_address,
                tls_spki_sha256: tls_spki_sha256.to_vec(),
                invite_code: invite.code.clone(),
                user_id: Some(pb::UserId {
                    username: invite.username.clone(),
                    server_id,
                }),
            };
            Ok(Bootstrap::Created(BootstrapInvite { invite, payload }))
        })
        .await
}
