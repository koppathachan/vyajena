//! Content bodies, sending with status, receiving in `server_seq` order and
//! notices (task group 6).
//!
//! Task group 5 only needs the receive path's core so that the engine can
//! follow the group in order: [`receive_application`] decrypts an
//! application message and stores it before the engine acks it. Decoding the
//! content, checking the sender and telling the UI are task 6.1 and 6.3.

use std::time::SystemTime;

use openmls::prelude::tls_codec::Deserialize as _;
use openmls::prelude::{MlsGroup, MlsMessageIn, ProcessedMessageContent};

use crate::engine::EngineError;
use crate::events::DeviceId;
use crate::identity::certificate::from_timestamp;
use crate::pb;
use crate::store::{IncomingMessage, Store};

/// Decrypts an `APPLICATION` envelope and stores it, inside the caller's
/// store transaction. A message that cannot be decrypted or has no Pi-set
/// sender is logged and skipped (task 6.3 decides how the UI shows it).
pub(crate) fn receive_application(
    store: &Store,
    group: &mut MlsGroup,
    envelope: &pb::Envelope,
    now: SystemTime,
) -> Result<(), EngineError> {
    let seq = envelope.server_seq;
    let Some(message) = MlsMessageIn::tls_deserialize_exact(envelope.ciphertext.as_slice())
        .ok()
        .and_then(|m| m.try_into_protocol_message().ok())
    else {
        tracing::warn!(server_seq = seq, "an application message does not parse");
        return Ok(());
    };
    let processed = match group.process_message(store.mls(), message) {
        Ok(processed) => processed,
        Err(error) => {
            tracing::warn!(server_seq = seq, %error, "cannot decrypt an application message");
            return Ok(());
        }
    };
    let ProcessedMessageContent::ApplicationMessage(content) = processed.into_content() else {
        tracing::warn!(
            server_seq = seq,
            "an APPLICATION envelope without an application message"
        );
        return Ok(());
    };
    let Some(sender) = envelope.sender.as_ref() else {
        return Ok(());
    };
    let (Some(user), Ok(device)) = (
        sender.user_id.as_ref(),
        DeviceId::try_from(sender.device_id.as_slice()),
    ) else {
        return Ok(());
    };
    let Ok(msg_id) = <[u8; 16]>::try_from(envelope.msg_id.as_slice()) else {
        return Ok(());
    };
    let sender_user = format!("{}@{}", user.username, user.server_id);
    store.insert_incoming(&IncomingMessage {
        server_seq: seq,
        msg_id,
        sender_user: &sender_user,
        sender_device: device,
        content: &content.into_bytes(),
        accepted_at: envelope
            .accepted_at
            .as_ref()
            .map(from_timestamp)
            .unwrap_or(now),
        received_at: now,
    })?;
    Ok(())
}
