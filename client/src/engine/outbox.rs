//! Sending application messages with status (client-runtime: Send status;
//! Local message store; client-core task 6.2), and telling the UI what was
//! received (client-runtime: Events in order; task 6.3).
//!
//! Own messages are stored with their plaintext as `sending` before
//! anything is sent ([`crate::Client::send_text`] and friends). The engine
//! sends every stored `sending` message in the order it was written:
//!
//! - one that was sent before (its ciphertext and epoch are stored) goes out
//!   again byte for byte, so after a reconnect or a crash the Pi's dedup
//!   answers with the original result (message-delivery: Idempotent sends);
//! - one never sent, or rejected with `STALE_EPOCH` (which leaves no dedup
//!   record), is encrypted in the group's current epoch with the same
//!   `msg_id`, and its ciphertext stored in the same transaction.
//!
//! Nothing is sent while an own commit waits for its result or envelopes are
//! held (it would be in the old epoch), while the group is behind the epoch
//! the Pi last named, or while this device is stalled. OpenMLS encrypts
//! nothing while proposals are pending (the Pi's inactivity Removes): then
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
            Received::GroupSettings { name, previous, by } => {
                self.emit(Event::GroupSettingsChanged { name, previous, by });
            }
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

    /// qa-fixes D2: a commit added devices, which never saw the group name or
    /// the profiles (they travel only when they change). Admin devices queue
    /// the current `GroupSettings` if a name is set; every device queues its
    /// own user's `Profile` if one is set. They go out through the outbox like
    /// any own message; receivers ignore an equal value.
    pub(super) fn resend_state(&self) -> Result<(), EngineError> {
        let now = self.now();
        let mut contents = Vec::new();
        if self.account.is_admin
            && let Some(name) = self.store().group_name()?
        {
            match crate::messaging::encode_group_settings(Some(&name), now) {
                Ok(content) => contents.push(content),
                Err(error) => tracing::warn!(%error, "cannot re-send the group settings"),
            }
        }
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
            // nothing of its chat goes out until the Pi has it.
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
                            self.push_job(Job::CommitPiRemovesIn {
                                group_id: group_id.clone(),
                            });
                        } else {
                            self.push_job(Job::CommitPiRemoves);
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

    /// An own profile or group settings the Pi accepted applies here too
    /// (the user's other devices apply it when they receive it).
    fn apply_own(&self, id: MessageId) -> Result<(), EngineError> {
        let Some(stored) = self.store().message(id)? else {
            return Ok(());
        };
        let now = self.now();
        match crate::messaging::decode(&stored.content).body {
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
            Decoded::GroupSettings { name } if self.account.is_admin => {
                let previous = self.store().group_name()?;
                self.store().set_group_name(name.as_deref(), now)?;
                if previous != name {
                    self.emit(Event::GroupSettingsChanged {
                        name,
                        previous,
                        by: self.user(),
                    });
                }
            }
            _ => {}
        }
        Ok(())
    }
}
