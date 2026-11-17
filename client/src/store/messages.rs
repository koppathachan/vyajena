//! client-runtime: Local message store. Own messages are stored with their
//! plaintext before sending; received messages before they are acked.

use std::time::SystemTime;

use prost::Message as _;
use rusqlite::{OptionalExtension, Row, params};

use super::{Store, StoreError};
use crate::clock::{from_unix_millis, unix_millis};
use crate::events::{DeviceId, MessageId, SendStatus};

/// Length of a wire `msg_id`.
const MSG_ID_LEN: usize = protocol::message_delivery::MSG_ID_LEN;

/// An own message, stored before it is sent.
#[derive(Debug, Clone, Copy)]
pub struct OutgoingMessage<'a> {
    /// The client-chosen wire msg_id, unique for this device.
    pub msg_id: [u8; MSG_ID_LEN],
    /// This device's user, `<username>@<server_id>`.
    pub sender_user: &'a str,
    /// This device.
    pub sender_device: DeviceId,
    /// The encoded `Content` (plaintext).
    pub content: &'a [u8],
    /// Now, on the local clock.
    pub created_at: SystemTime,
}

/// A message received from the Pi and decrypted.
#[derive(Debug, Clone, Copy)]
pub struct IncomingMessage<'a> {
    /// Its place in the group's sequence.
    pub server_seq: u64,
    /// The sender's wire msg_id.
    pub msg_id: [u8; MSG_ID_LEN],
    /// The sending user.
    pub sender_user: &'a str,
    /// The sending device.
    pub sender_device: DeviceId,
    /// The decrypted, encoded `Content`.
    pub content: &'a [u8],
    /// When the Pi accepted it.
    pub accepted_at: SystemTime,
    /// Now, on the local clock.
    pub received_at: SystemTime,
    /// Whether it mentions this device's user or everyone and comes from
    /// another user: stored as an unseen mention (client-runtime: Unseen
    /// mentions).
    pub mentions_me: bool,
}

/// What [`Store::insert_incoming`] did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InsertOutcome {
    /// Stored as a new message.
    Inserted(MessageId),
    /// Already stored (a redelivery after a crash before the ack): nothing
    /// changed, and the UI must not see it again.
    Duplicate(MessageId),
}

/// A message as stored.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredMessage {
    /// Local id.
    pub id: MessageId,
    /// The sending user, `<username>@<server_id>`.
    pub sender_user: String,
    /// The sending device.
    pub sender_device: DeviceId,
    /// The wire msg_id.
    pub msg_id: [u8; MSG_ID_LEN],
    /// Whether this device sent it.
    pub outgoing: bool,
    /// The encoded `Content` (plaintext).
    pub content: Vec<u8>,
    /// When this device stored it (local clock).
    pub created_at: SystemTime,
    /// Its place in the group's sequence, once the Pi accepted it.
    pub server_seq: Option<u64>,
    /// When the Pi accepted it.
    pub accepted_at: Option<SystemTime>,
    /// Own messages: where it stands. `None` for received messages.
    pub status: Option<SendStatus>,
    /// Own message in flight: the epoch of the last envelope sent.
    pub sent_epoch: Option<u64>,
    /// Own message in flight: the exact ciphertext last sent.
    pub ciphertext: Option<Vec<u8>>,
    /// The chat it is in: 0 is the main group, else a DM's chat id (change
    /// `dms`).
    pub chat: i64,
}

const COLUMNS: &str = "local_id, sender_user, sender_device, msg_id, outgoing, content, \
                       created_at, server_seq, accepted_at, status, failure, sent_epoch, \
                       ciphertext, chat_id";

/// The `kind` column for an encoded `Content`: `profile` and `settings`
/// bodies change state; Text, unknown bodies and anything that does not
/// decode are `chat`, shown in the history.
fn kind_of(content: &[u8]) -> &'static str {
    use crate::pb::content::Body;
    match crate::pb::Content::decode(content).map(|c| c.body) {
        Ok(Some(Body::Profile(_))) => "profile",
        Ok(Some(Body::GroupSettings(_))) => "settings",
        _ => "chat",
    }
}

fn to_i64(value: u64) -> Result<i64, StoreError> {
    i64::try_from(value).map_err(|_| StoreError::Inconsistent(format!("{value} exceeds i64")))
}

fn to_u64(value: Option<i64>, what: &str) -> Result<Option<u64>, StoreError> {
    value
        .map(|v| u64::try_from(v).map_err(|_| StoreError::Inconsistent(format!("negative {what}"))))
        .transpose()
}

fn fixed<const N: usize>(bytes: Vec<u8>, what: &str) -> Result<[u8; N], StoreError> {
    bytes
        .try_into()
        .map_err(|_| StoreError::Inconsistent(format!("{what} is not {N} bytes")))
}

/// The raw columns of one row, converted by [`decode`].
struct RawRow {
    local_id: i64,
    sender_user: String,
    sender_device: Vec<u8>,
    msg_id: Vec<u8>,
    outgoing: bool,
    content: Vec<u8>,
    created_at: i64,
    server_seq: Option<i64>,
    accepted_at: Option<i64>,
    status: String,
    failure: Option<String>,
    sent_epoch: Option<i64>,
    ciphertext: Option<Vec<u8>>,
    chat: i64,
}

fn raw(row: &Row<'_>) -> rusqlite::Result<RawRow> {
    Ok(RawRow {
        local_id: row.get(0)?,
        sender_user: row.get(1)?,
        sender_device: row.get(2)?,
        msg_id: row.get(3)?,
        outgoing: row.get(4)?,
        content: row.get(5)?,
        created_at: row.get(6)?,
        server_seq: row.get(7)?,
        accepted_at: row.get(8)?,
        status: row.get(9)?,
        failure: row.get(10)?,
        sent_epoch: row.get(11)?,
        ciphertext: row.get(12)?,
        chat: row.get(13)?,
    })
}

fn decode(row: RawRow) -> Result<StoredMessage, StoreError> {
    let status = match row.status.as_str() {
        "received" => None,
        "sending" => Some(SendStatus::Sending),
        "accepted" => Some(SendStatus::Accepted),
        "failed" => Some(SendStatus::Failed {
            reason: row.failure.unwrap_or_default(),
        }),
        other => return Err(StoreError::Inconsistent(format!("message status {other}"))),
    };
    Ok(StoredMessage {
        id: MessageId(row.local_id),
        sender_user: row.sender_user,
        sender_device: fixed(row.sender_device, "sender_device")?,
        msg_id: fixed(row.msg_id, "msg_id")?,
        outgoing: row.outgoing,
        content: row.content,
        created_at: from_unix_millis(row.created_at),
        server_seq: to_u64(row.server_seq, "server_seq")?,
        accepted_at: row.accepted_at.map(from_unix_millis),
        status,
        sent_epoch: to_u64(row.sent_epoch, "sent_epoch")?,
        ciphertext: row.ciphertext,
        chat: row.chat,
    })
}

impl Store {
    /// Stores an own message as sending, before anything is sent
    /// (client-runtime: Local message store).
    pub fn insert_outgoing(&self, message: &OutgoingMessage<'_>) -> Result<MessageId, StoreError> {
        self.insert_outgoing_in(super::MAIN_CHAT, message)
    }

    /// [`Store::insert_outgoing`] in chat `chat` (change `dms`).
    pub fn insert_outgoing_in(
        &self,
        chat: i64,
        message: &OutgoingMessage<'_>,
    ) -> Result<MessageId, StoreError> {
        let id = self.db.with(|conn| {
            conn.execute(
                "INSERT INTO messages (sender_user, sender_device, msg_id, outgoing, content,
                                       created_at, status, kind, chat_id)
                 VALUES (?1, ?2, ?3, 1, ?4, ?5, 'sending', ?6, ?7)",
                params![
                    message.sender_user,
                    message.sender_device,
                    message.msg_id,
                    message.content,
                    unix_millis(message.created_at),
                    kind_of(message.content),
                    chat,
                ],
            )?;
            Ok::<_, StoreError>(conn.last_insert_rowid())
        })?;
        tracing::debug!(id, size = message.content.len(), "stored own message");
        Ok(MessageId(id))
    }

    /// Records the epoch and exact ciphertext of the envelope just sent for an
    /// own message that is still sending, so a resend after a crash repeats
    /// the same bytes.
    pub fn record_sent(
        &self,
        id: MessageId,
        epoch: u64,
        ciphertext: &[u8],
    ) -> Result<(), StoreError> {
        let epoch = to_i64(epoch)?;
        self.update_sending(
            id,
            "UPDATE messages SET sent_epoch = ?2, ciphertext = ?3
             WHERE local_id = ?1 AND status = 'sending'",
            params![id.0, epoch, ciphertext],
        )
    }

    /// Drops the ciphertext of an own message the Pi rejected with
    /// `STALE_EPOCH` (which leaves no dedup record), so it is encrypted again
    /// in the new epoch with the same msg_id (client-runtime: Send status).
    pub fn clear_sent(&self, id: MessageId) -> Result<(), StoreError> {
        self.update_sending(
            id,
            "UPDATE messages SET sent_epoch = NULL, ciphertext = NULL
             WHERE local_id = ?1 AND status = 'sending'",
            params![id.0],
        )
    }

    /// Marks an own message accepted at `server_seq` (client-runtime: Send
    /// status) and drops its in-flight ciphertext.
    pub fn mark_accepted(
        &self,
        id: MessageId,
        server_seq: u64,
        accepted_at: SystemTime,
    ) -> Result<(), StoreError> {
        let server_seq = to_i64(server_seq)?;
        self.update_sending(
            id,
            "UPDATE messages SET status = 'accepted', server_seq = ?2, accepted_at = ?3,
                                 sent_epoch = NULL, ciphertext = NULL
             WHERE local_id = ?1 AND status = 'sending'",
            params![id.0, server_seq, unix_millis(accepted_at)],
        )
    }

    /// Marks an own message failed with `reason` (client-runtime: Send
    /// status).
    pub fn mark_failed(&self, id: MessageId, reason: &str) -> Result<(), StoreError> {
        self.update_sending(
            id,
            "UPDATE messages SET status = 'failed', failure = ?2,
                                 sent_epoch = NULL, ciphertext = NULL
             WHERE local_id = ?1 AND status = 'sending'",
            params![id.0, reason],
        )
    }

    fn update_sending(
        &self,
        id: MessageId,
        sql: &str,
        params: impl rusqlite::Params,
    ) -> Result<(), StoreError> {
        let changed = self.db.with(|conn| conn.execute(sql, params))?;
        if changed == 1 {
            Ok(())
        } else {
            Err(StoreError::Inconsistent(format!(
                "message {} is not an own message that is sending",
                id.0
            )))
        }
    }

    /// Own messages still sending, oldest first: what to resend after a
    /// restart or reconnect, with the same msg_id.
    pub fn outgoing_in_flight(&self) -> Result<Vec<StoredMessage>, StoreError> {
        self.query(
            &format!("SELECT {COLUMNS} FROM messages WHERE status = 'sending' ORDER BY local_id"),
            [],
        )
    }

    /// Stores a received message, unless that `server_seq` (or that sender's
    /// msg_id) is already stored (client-runtime: Local message store). Call
    /// it before acking `server_seq`.
    pub fn insert_incoming(
        &self,
        message: &IncomingMessage<'_>,
    ) -> Result<InsertOutcome, StoreError> {
        self.insert_incoming_in(super::MAIN_CHAT, message)
    }

    /// [`Store::insert_incoming`] in chat `chat` (change `dms`).
    pub fn insert_incoming_in(
        &self,
        chat: i64,
        message: &IncomingMessage<'_>,
    ) -> Result<InsertOutcome, StoreError> {
        let server_seq = to_i64(message.server_seq)?;
        self.transaction(|| {
            let existing: Option<i64> = self.db.with(|conn| {
                conn.query_row(
                    "SELECT local_id FROM messages
                     WHERE server_seq = ?1 OR (sender_device = ?2 AND msg_id = ?3)",
                    params![server_seq, message.sender_device, message.msg_id],
                    |row| row.get(0),
                )
                .optional()
            })?;
            if let Some(id) = existing {
                tracing::debug!(server_seq, "message already stored");
                return Ok(InsertOutcome::Duplicate(MessageId(id)));
            }
            let id = self.db.with(|conn| {
                conn.execute(
                    "INSERT INTO messages (sender_user, sender_device, msg_id, outgoing, content,
                                           created_at, server_seq, accepted_at, status, kind,
                                           mention_unseen, chat_id)
                     VALUES (?1, ?2, ?3, 0, ?4, ?5, ?6, ?7, 'received', ?8, ?9, ?10)",
                    params![
                        message.sender_user,
                        message.sender_device,
                        message.msg_id,
                        message.content,
                        unix_millis(message.received_at),
                        server_seq,
                        unix_millis(message.accepted_at),
                        kind_of(message.content),
                        message.mentions_me,
                        chat,
                    ],
                )?;
                Ok::<_, StoreError>(conn.last_insert_rowid())
            })?;
            tracing::debug!(
                id,
                server_seq,
                size = message.content.len(),
                "stored message"
            );
            Ok(InsertOutcome::Inserted(MessageId(id)))
        })
    }

    /// One message by local id.
    pub fn message(&self, id: MessageId) -> Result<Option<StoredMessage>, StoreError> {
        Ok(self
            .query(
                &format!("SELECT {COLUMNS} FROM messages WHERE local_id = ?1"),
                params![id.0],
            )?
            .pop())
    }

    /// The newest `limit` chat messages (Text and bodies this version does
    /// not know; not profiles or group settings) in display order: by
    /// `server_seq`, then own messages the Pi has not accepted, in the order
    /// they were written.
    pub fn history(&self, limit: usize) -> Result<Vec<StoredMessage>, StoreError> {
        self.history_in(super::MAIN_CHAT, limit)
    }

    /// [`Store::history`] of chat `chat` (change `dms`).
    pub fn history_in(&self, chat: i64, limit: usize) -> Result<Vec<StoredMessage>, StoreError> {
        let limit = i64::try_from(limit).unwrap_or(i64::MAX);
        let mut newest_first = self.query(
            &format!(
                "SELECT {COLUMNS} FROM messages WHERE kind = 'chat' AND chat_id = ?2
                 ORDER BY server_seq IS NULL DESC, server_seq DESC, local_id DESC
                 LIMIT ?1"
            ),
            params![limit, chat],
        )?;
        newest_first.reverse();
        Ok(newest_first)
    }

    /// The local id of the chat message `sender_device` sent with `msg_id`,
    /// if stored (client-runtime: Reply lookup).
    pub fn chat_message_by_ref(
        &self,
        sender_device: &DeviceId,
        msg_id: &[u8; MSG_ID_LEN],
    ) -> Result<Option<MessageId>, StoreError> {
        let id: Option<i64> = self.db.with(|conn| {
            conn.query_row(
                "SELECT local_id FROM messages
                 WHERE sender_device = ?1 AND msg_id = ?2 AND kind = 'chat'",
                params![sender_device, msg_id],
                |row| row.get(0),
            )
            .optional()
        })?;
        Ok(id.map(MessageId))
    }

    /// The messages whose mention of this device's user is not seen yet, in
    /// chat order (client-runtime: Unseen mentions).
    pub fn unseen_mentions(&self) -> Result<Vec<MessageId>, StoreError> {
        self.unseen_mentions_in(super::MAIN_CHAT)
    }

    /// [`Store::unseen_mentions`] in chat `chat` (change `dms`).
    pub fn unseen_mentions_in(&self, chat: i64) -> Result<Vec<MessageId>, StoreError> {
        let ids = self.db.with(|conn| {
            let mut statement = conn.prepare(
                "SELECT local_id FROM messages WHERE mention_unseen = 1 AND chat_id = ?1
                 ORDER BY server_seq IS NULL, server_seq, local_id",
            )?;
            let rows = statement.query_map(params![chat], |row| row.get::<_, i64>(0))?;
            rows.collect::<rusqlite::Result<Vec<i64>>>()
        })?;
        Ok(ids.into_iter().map(MessageId).collect())
    }

    /// Marks one message's mention seen; `false` if it had none unseen.
    pub fn mark_mention_seen(&self, id: MessageId) -> Result<bool, StoreError> {
        let changed = self.db.with(|conn| {
            conn.execute(
                "UPDATE messages SET mention_unseen = 0
                 WHERE local_id = ?1 AND mention_unseen = 1",
                params![id.0],
            )
        })?;
        Ok(changed == 1)
    }

    /// The highest `server_seq` of any stored message.
    pub fn highest_message_seq(&self) -> Result<Option<u64>, StoreError> {
        let seq: Option<i64> = self.db.with(|conn| {
            conn.query_row("SELECT max(server_seq) FROM messages", [], |row| row.get(0))
        })?;
        to_u64(seq, "server_seq")
    }

    fn query(
        &self,
        sql: &str,
        params: impl rusqlite::Params,
    ) -> Result<Vec<StoredMessage>, StoreError> {
        let rows = self.db.with(|conn| {
            let mut statement = conn.prepare(sql)?;
            let rows = statement.query_map(params, raw)?;
            rows.collect::<rusqlite::Result<Vec<RawRow>>>()
        })?;
        rows.into_iter().map(decode).collect()
    }
}
