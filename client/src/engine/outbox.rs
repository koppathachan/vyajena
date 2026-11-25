//! Sending application messages with status (client-runtime: Send status;
//! Local message store; client-core task 6.2), and telling the UI what was
//! received (client-runtime: Events in order; task 6.3).
//!
//! Own messages are stored with their plaintext as `sending` before
//! anything is sent ([`crate::Client::send_text`] and friends). The engine
//! sends every stored `sending` message in the order it was written:
//!
//! - one that was sent before (its ciphertext and epoch are stored) goes out
//!   again byte for byte, so after a reconnect or a crash the server's dedup
//!   answers with the original result (message-delivery: Idempotent sends);
//! - one never sent, or rejected with `STALE_EPOCH` (which leaves no dedup
//!   record), is encrypted in the group's current epoch with the same
//!   `msg_id`, and its ciphertext stored in the same transaction.
//!
//! Nothing is sent while an own commit waits for its result or envelopes are
//! held (it would be in the old epoch), while the group is behind the epoch
//! the server last named, or while this device is stalled. OpenMLS encrypts
//! nothing while proposals are pending (the server's inactivity Removes): then
//! this device commits them first, which commit authority allows any member.

use std::sync::Arc;

use super::{Engine, EngineError, Job};
use crate::events::{Event, MessageId, SendStatus};
use crate::messaging::{Decoded, Received};
use crate::mls::{MlsError, group as mls_group};
use crate::pb;
use crate::pb::client_frame::Frame as Out;
use crate::pb::envelope::Kind;
use crate::store::{OutgoingMessage, StoredMessage};

impl Engine {
    /// Emits what receiving an application message produced.
    pub(super) fn emit_received(&self, received: Received) {
        match received {
            Received::Message(message) => self.emit(Event::Message(message)),
            Received::Profile { user, profile } => self.emit(Event::ProfileChanged {
                user,
                display_name: profile.display_name,
                about: profile.about,
            }),
            Received::ChatRenamed => self.emit(Event::ChatsChanged),
            Received::Unreadable {
                sender,
                accepted_at,
                chat,
            } => self.emit(Event::Unreadable {
                sender,
                accepted_at,
                chat,
            }),
            Received::Nothing => {}
        }
    }

    /// multi-group: queues the named group `group_id`'s current name as a
    /// `GroupSettings` in its chat (an admin device after devices were added to
    /// it); nothing for the lobby or when no name is set.
    pub(super) fn resend_named_name(&self, group_id: &[u8]) -> Result<(), EngineError> {
        let Some(row) = self.store().dm_by_group(group_id)? else {
            return Ok(());
        };
        let Some(name) = row.name.filter(|_| row.named) else {
            return Ok(());
        };
        let now = self.now();
        let content = match crate::messaging::encode_group_settings(Some(&name), now) {
            Ok(content) => content,
            Err(error) => {
                tracing::warn!(%error, "cannot re-send the group's name");
                return Ok(());
            }
        };
        let user = self.user().to_string();
        self.store().insert_outgoing_in(
            row.chat,
            &OutgoingMessage {
                msg_id: super::random_msg_id()?,
                sender_user: &user,
                sender_device: self.account.device_id,
                content: &content,
                created_at: now,
            },
        )?;
        Ok(())
    }

    /// qa-fixes D2: a commit added devices, which never saw the profiles
    /// (they travel only when they change). Every device queues its own user's
    /// `Profile` if one is set. They go out through the outbox like any own
    /// message; receivers ignore an equal value. (The lobby has no settings
    /// to re-send: its name is fixed.)
    pub(super) fn resend_state(&self) -> Result<(), EngineError> {
        let now = self.now();
        let mut contents = Vec::new();
        if let Some(profile) = self.store().profile(&self.user().to_string())?
            && (profile.display_name.is_some() || profile.about.is_some())
        {
            match crate::messaging::encode_profile(&profile, now) {
                Ok(content) => contents.push(content),
                Err(error) => tracing::warn!(%error, "cannot re-send the profile"),
            }
        }
        let user = self.user().to_string();
        for content in contents {
            let msg_id = super::random_msg_id()?;
            let id = self.store().insert_outgoing(&OutgoingMessage {
                msg_id,
                sender_user: &user,
                sender_device: self.account.device_id,
                content: &content,
                created_at: now,
            })?;
            tracing::debug!(id = id.0, "queued a re-send of the group state");
        }
        Ok(())
    }

    /// Sends the stored own messages that are not in flight, oldest first,
    /// each in its chat's group (change `dms`). A chat that cannot send now
    /// holds its later messages, so their order stays; other chats go on.
    pub(super) async fn flush_outbox(&mut self) -> Result<(), EngineError> {
        if self.link.is_none() || self.inflight.is_some() || !self.deferred.is_empty() {
            return Ok(());
        }
        let waiting: Vec<StoredMessage> = self
            .store()
            .outgoing_in_flight()?
            .into_iter()
            .filter(|m| !self.apps_in_flight.contains_key(&m.msg_id))
            .collect();
        let mut held = std::collections::BTreeSet::new();
        for message in waiting {
            if held.contains(&message.chat) {
                continue;
            }
            let Some(group_id) = self.sendable_group(message.chat)? else {
                held.insert(message.chat);
                continue;
            };
            // A queued image: its blob goes up first, in a task of its own, and
            // nothing of its chat goes out until the server has it.
            if self.store().is_blob_pending(message.id)? {
                held.insert(message.chat);
                self.start_upload(&message, group_id)?;
                continue;
            }
            let Some(group) = self.group_ref(&group_id) else {
                held.insert(message.chat);
                continue;
            };
            let epoch = group.epoch().as_u64();
            let (sent_epoch, ciphertext) = match (message.sent_epoch, message.ciphertext.clone()) {
                (Some(sent_epoch), Some(ciphertext)) => (sent_epoch, ciphertext),
                _ => {
                    // After STALE_EPOCH, wait for the commits that moved the
                    // group on; keep the order of own messages.
                    if epoch < self.server_epochs.get(&group_id).copied().unwrap_or(0) {
                        held.insert(message.chat);
                        continue;
                    }
                    if group.pending_proposals().next().is_some() {
                        if self.dms.contains_key(&group_id) {
                            self.push_job(Job::CommitServerRemovesIn {
                                group_id: group_id.clone(),
                            });
                        } else {
                            self.push_job(Job::CommitServerRemoves);
                        }
                        held.insert(message.chat);
                        continue;
                    }
                    match self.encrypt(&message, &group_id, epoch)? {
                        Some(ciphertext) => (epoch, ciphertext),
                        None => continue,
                    }
                }
            };
            let envelope = pb::Envelope {
                msg_id: message.msg_id.to_vec(),
                group_id,
                epoch: sent_epoch,
                kind: Kind::Application as i32,
                ciphertext,
                // message-deletion: a Delete carries its server-visible target,
                // so the server retracts the target's undelivered copies.
                deletes: crate::messaging::delete_target(&message.content),
                ..Default::default()
            };
            self.send(Out::Envelope(envelope)).await?;
            tracing::debug!(id = message.id.0, epoch = sent_epoch, "sent an own message");
            self.apps_in_flight.insert(message.msg_id, message.id);
        }
        Ok(())
    }

    /// The group an own message in `chat` can go out in now: the main group
    /// unless this device stalled; a DM once it is joined, open, not
    /// stalled, and one of the peer's devices is in it (app-ui: Open a DM,
    /// "sent once Bob's phone is added").
    fn sendable_group(&self, chat: i64) -> Result<Option<Vec<u8>>, EngineError> {
        if chat == crate::store::MAIN_CHAT {
            let Some(row) = self.store().group()? else {
                return Ok(None);
            };
            return Ok(row.stalled.is_none().then_some(row.group_id));
        }
        let Some(dm) = self.store().dm_by_chat(chat)? else {
            return Ok(None);
        };
        if !dm.joined || dm.closed_at.is_some() || dm.state.stalled.is_some() {
            return Ok(None);
        }
        if dm.named {
            // multi-group: a named group sends once someone else is in it.
            let others = self
                .dms
                .get(&dm.group_id)
                .is_some_and(|g| g.members().count() > 1);
            return Ok(others.then_some(dm.group_id));
        }
        let Some(peer) = crate::messaging::parse_user(&dm.peer) else {
            return Ok(None);
        };
        let has_peer = self
            .dms
            .get(&dm.group_id)
            .is_some_and(|g| !crate::membership::leaves_of_user(g, &peer).is_empty());
        Ok(has_peer.then_some(dm.group_id))
    }

    /// Encrypts `message` in the current epoch of `group_id` and stores the
    /// ciphertext with it, in one transaction. `None` if it is too large to
    /// send: it is marked failed (message-content: Text only).
    fn encrypt(
        &mut self,
        message: &StoredMessage,
        group_id: &[u8],
        epoch: u64,
    ) -> Result<Option<Vec<u8>>, EngineError> {
        let store = Arc::clone(&self.shared.store);
        let signer = self.keys.mls_signer();
        let Some(group) = super::group_in(&mut self.group, &mut self.dms, group_id) else {
            return Ok(None);
        };
        let cap = protocol::wire_transport::MAX_APPLICATION_CIPHERTEXT_BYTES;
        let encrypted = store.transaction(|| -> Result<Vec<u8>, EngineError> {
            let out = group
                .create_message(store.mls(), &signer, &message.content)
                .map_err(|e| MlsError::new("encrypting a message", e))?;
            let bytes = mls_group::wire(&out)?;
            if bytes.len() <= cap {
                store.record_sent(message.id, epoch, &bytes)?;
            }
            Ok(bytes)
        });
        let bytes = match encrypted {
            Ok(bytes) => bytes,
            Err(error) => {
                self.reload_group_in(group_id)?;
                return Err(error);
            }
        };
        if bytes.len() > cap {
            tracing::warn!(
                id = message.id.0,
                size = bytes.len(),
                "an own message is too large"
            );
            self.fail(message.id, "the message is too long")?;
            return Ok(None);
        }
        Ok(Some(bytes))
    }

    pub(super) fn fail(&self, id: MessageId, reason: &str) -> Result<(), EngineError> {
        self.store().mark_failed(id, reason)?;
        self.emit(Event::SendStatus {
            id,
            status: SendStatus::Failed {
                reason: reason.to_owned(),
            },
        });
        Ok(())
    }

    /// The result for an own application message, if `result` is one.
    pub(super) fn on_app_result(&mut self, result: &pb::SendResult) -> Result<bool, EngineError> {
        let Ok(msg_id) = <[u8; 16]>::try_from(result.msg_id.as_slice()) else {
            return Ok(false);
        };
        let Some(id) = self.apps_in_flight.remove(&msg_id) else {
            return Ok(false);
        };
        match &result.result {
            Some(pb::send_result::Result::Accepted(accepted)) => {
                let at = accepted
                    .accepted_at
                    .as_ref()
                    .map(crate::identity::certificate::from_timestamp)
                    .unwrap_or_else(|| self.now());
                self.store().mark_accepted(id, accepted.server_seq, at)?;
                tracing::debug!(
                    id = id.0,
                    server_seq = accepted.server_seq,
                    "own message accepted"
                );
                self.emit(Event::SendStatus {
                    id,
                    status: SendStatus::Accepted,
                });
                self.apply_own(id)?;
            }
            Some(pb::send_result::Result::Rejected(rejected)) => {
                let reason = pb::RejectionReason::try_from(rejected.reason)
                    .unwrap_or(pb::RejectionReason::Unspecified);
                if reason == pb::RejectionReason::StaleEpoch {
                    // client-runtime: Send status. Catch up, then encrypt
                    // again with the same msg_id.
                    tracing::info!(
                        id = id.0,
                        current_epoch = rejected.current_epoch,
                        "own message was stale; resending after catching up"
                    );
                    self.store().clear_sent(id)?;
                    let chat = self.store().message(id)?.map_or(0, |m| m.chat);
                    if let Some(group_id) = self.sendable_group(chat)? {
                        let hint = self.server_epochs.entry(group_id).or_default();
                        *hint = (*hint).max(rejected.current_epoch);
                    }
                } else {
                    tracing::warn!(
                        id = id.0,
                        reason = reason.as_str_name(),
                        "own message rejected"
                    );
                    self.fail(id, reason.as_str_name())?;
                }
            }
            None => {}
        }
        Ok(true)
    }

    /// An own profile, group settings or delete the server accepted applies
    /// here too (the user's other devices apply it when they receive it).
    fn apply_own(&mut self, id: MessageId) -> Result<(), EngineError> {
        let Some(stored) = self.store().message(id)? else {
            return Ok(());
        };
        let now = self.now();
        match crate::messaging::decode(&stored.content).body {
            Decoded::Delete(del) => self.apply_own_delete(&stored, del)?,
            Decoded::Profile(profile) => {
                let user = self.user();
                let stored = self.store().profile(&user.to_string())?;
                self.store().set_profile(&user.to_string(), &profile, now)?;
                if stored.unwrap_or_default() != profile {
                    self.emit(Event::ProfileChanged {
                        user,
                        display_name: profile.display_name,
                        about: profile.about,
                    });
                }
            }
            Decoded::GroupSettings { name: Some(name) } if self.account.is_admin => {
                // multi-group: a named group's name applies here too; the
                // lobby's name is fixed.
                if stored.chat != crate::store::MAIN_CHAT
                    && let Some(group_id) =
                        self.store().dm_by_chat(stored.chat)?.map(|g| g.group_id)
                    && self.store().set_named_name(&group_id, &name)?
                {
                    self.emit(Event::ChatsChanged);
                }
            }
            _ => {}
        }
        Ok(())
    }

    /// message-deletion: applies this device's own accepted delete — marks the
    /// target a tombstone (keeping the original for a DM reveal, dropping it for
    /// the lobby or a named group), emits the tombstone, and purges the blob of
    /// a deleted media message at once with `Blobs.DeleteBlob` (Deleting media
    /// and replies).
    fn apply_own_delete(
        &mut self,
        stored: &StoredMessage,
        del: crate::messaging::DeleteContent,
    ) -> Result<(), EngineError> {
        let is_group = if stored.chat == crate::store::MAIN_CHAT {
            true
        } else {
            self.store()
                .dm_by_chat(stored.chat)?
                .map(|dm| dm.named)
                .unwrap_or(false)
        };
        let by_admin = is_group
            && (del.target_user != self.user() || del.target_device != self.account.device_id);
        // Capture the target's blob before its plaintext may be replaced.
        let target_blob = match self
            .store()
            .chat_message_by_ref(&del.target_device, &del.target_msg_id)?
        {
            Some(target_id) => self
                .store()
                .message(target_id)?
                .and_then(|m| blob_of(&m.content)),
            None => None,
        };
        let replacement = is_group.then(|| stored.content.clone());
        let tombstone_id = match self.store().mark_deleted_by_ref(
            &del.target_device,
            &del.target_msg_id,
            by_admin,
            replacement.as_deref(),
        )? {
            Some(target_id) => {
                self.store().hide_message(stored.id)?;
                target_id
            }
            None => {
                self.store().mark_delete_tombstone(stored.id, by_admin)?;
                stored.id
            }
        };
        let own_user = self.user();
        if let Some(message) = self
            .store()
            .message(tombstone_id)?
            .and_then(|stored| crate::messaging::to_message(self.store(), &stored, &own_user))
        {
            self.emit(Event::Message(message));
        }
        // message-deletion: Deleting media and replies. Purge the one stored
        // blob at once; a no-op when it already expired.
        if let Some(blob_id) = target_blob
            && let Some(group_id) = self.group_id_of_chat(stored.chat)?
        {
            self.push_job(Job::PurgeBlob { group_id, blob_id });
        }
        Ok(())
    }

    /// The group id of chat `chat`: the main group, or a DM/named group's.
    fn group_id_of_chat(&self, chat: i64) -> Result<Option<Vec<u8>>, EngineError> {
        if chat == crate::store::MAIN_CHAT {
            Ok(self.store().group()?.map(|row| row.group_id))
        } else {
            Ok(self.store().dm_by_chat(chat)?.map(|dm| dm.group_id))
        }
    }
}

/// The blob id of an image or voice message, for a media delete to purge.
fn blob_of(content: &[u8]) -> Option<Vec<u8>> {
    match crate::messaging::decode(content).body {
        Decoded::Image(image) => Some(image.blob_id.to_vec()),
        Decoded::Audio(audio) => Some(audio.blob_id.to_vec()),
        Decoded::Video(video) => Some(video.blob_id.to_vec()),
        _ => None,
    }
}
