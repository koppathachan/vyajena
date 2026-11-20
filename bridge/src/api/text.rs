//! Text forms of invites, link requests and link approvals (flutter-app
//! design D5): a short prefix naming the kind, then the payload in base64url
//! without padding, as `client::identity::payload::to_text` writes it.
//!
//! Text without a prefix is accepted too: that is how the server prints its
//! bootstrap invite, which the admin pastes on their first device. A payload
//! of another kind is refused by its prefix, so pasting a link approval into
//! the invite box says so instead of failing to decode.

use client::identity::payload::{from_text, to_text};
use client::identity::{Invite, PayloadError};

use super::error::{CoreError, CoreErrorKind};
use super::types::InvitePreview;
use crate::map;

/// The prefix of an invite text.
pub const INVITE_PREFIX: &str = "vyajena-invite:";
/// The prefix of a link request text (shown by the new device).
pub const LINK_REQUEST_PREFIX: &str = "vyajena-link:";
/// The prefix of a link approval text (shown by the identity device).
pub const LINK_APPROVAL_PREFIX: &str = "vyajena-approval:";

const ALL: [(&str, &str); 3] = [
    (INVITE_PREFIX, "an invite"),
    (LINK_REQUEST_PREFIX, "a link request"),
    (LINK_APPROVAL_PREFIX, "a link approval"),
];

/// What an invite text names, without joining: the user it creates and the
/// relay's address (app-ui: Join from an invite). Fails with
/// [`CoreErrorKind::InvalidPayload`] for text that is not an invite.
#[flutter_rust_bridge::frb(sync)]
pub fn parse_invite(text: String) -> Result<InvitePreview, CoreError> {
    let invite = Invite::decode(&invite_bytes(&text)?).map_err(invalid)?;
    Ok(InvitePreview {
        user: map::user(&invite.user),
        relay_address: invite.relay_address,
    })
}

pub(crate) fn invite_text(bytes: &[u8]) -> String {
    format!("{INVITE_PREFIX}{}", to_text(bytes))
}

pub(crate) fn link_request_text(bytes: &[u8]) -> String {
    format!("{LINK_REQUEST_PREFIX}{}", to_text(bytes))
}

pub(crate) fn link_approval_text(bytes: &[u8]) -> String {
    format!("{LINK_APPROVAL_PREFIX}{}", to_text(bytes))
}

pub(crate) fn invite_bytes(text: &str) -> Result<Vec<u8>, CoreError> {
    payload_bytes(text, INVITE_PREFIX)
}

pub(crate) fn link_request_bytes(text: &str) -> Result<Vec<u8>, CoreError> {
    payload_bytes(text, LINK_REQUEST_PREFIX)
}

pub(crate) fn link_approval_bytes(text: &str) -> Result<Vec<u8>, CoreError> {
    payload_bytes(text, LINK_APPROVAL_PREFIX)
}

/// The payload bytes of `text`, which carries `prefix` or none at all.
fn payload_bytes(text: &str, prefix: &str) -> Result<Vec<u8>, CoreError> {
    let text = text.trim();
    let expected = ALL
        .iter()
        .find(|(p, _)| *p == prefix)
        .map_or("a payload", |(_, name)| name);
    if let Some(rest) = text.strip_prefix(prefix) {
        return from_text(rest).map_err(invalid);
    }
    if let Some((_, other)) = ALL.iter().find(|(p, _)| text.starts_with(p)) {
        return Err(CoreError::new(
            CoreErrorKind::InvalidPayload,
            format!("this is {other}, not {expected}"),
        ));
    }
    from_text(text).map_err(invalid)
}

pub(crate) fn invalid(error: PayloadError) -> CoreError {
    CoreError::new(CoreErrorKind::InvalidPayload, error.to_string())
}
