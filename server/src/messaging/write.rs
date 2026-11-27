//! The one write path per accepted envelope (server-core design D4):
//! checks, then in one transaction on the store's single writer the next
//! `server_seq`, the envelope, its deliveries (queue entries, commit-log
//! entries, Welcomes), the dedup record and the new group state. `Accepted`
//! is sent only after that transaction commits (message-delivery: Accepted
//! means durable).
//!
//! Check order, each failing check rejecting the envelope with nothing
//! stored: `msg_id` length and kind (`MALFORMED`, a client `WELCOME`
//! included), size (`TOO_LARGE`), a Welcome bundle on anything but a
//! `COMMIT` (`MALFORMED`), dedup (`DUPLICATE_MISMATCH`, or the original
//! result for an identical retry), the group id and MLS framing (`MALFORMED`),
//! the strict epoch rule (`STALE_EPOCH`), the sender being an active member
//! (`POLICY_VIOLATION`), then the handshake checks of
//! [`GroupState::check_commit`] / [`GroupState::check_proposal`] and the
//! Welcome bundle (`MALFORMED`).

use std::time::SystemTime;

use openmls::prelude::tls_codec::Deserialize as _;
use openmls::prelude::{ContentType, MlsMessageBodyIn, MlsMessageIn, ProtocolMessage};
use prost::Message as _;
use protocol::device_identity::DeviceId;
use protocol::message_deletion::{DeleteAuthority, may_delete};
use protocol::message_delivery::MSG_ID_LEN;
use protocol::wire_transport::{MAX_APPLICATION_CIPHERTEXT_BYTES, MAX_HANDSHAKE_ENVELOPE_BYTES};
use rusqlite::Transaction;
use sha2::{Digest, Sha256};

use super::log::{self, DedupRow, NewEnvelope};
use crate::auth::Caller;
use crate::membership::group::{self, GroupKind};
use crate::membership::{AcceptedCommit, GroupState, inactivity, joins};
use crate::pb;
use crate::pb::RejectionReason;
use crate::pb::envelope::Kind;
use crate::runtime::ServerKeys;
use crate::store::StoreError;

/// What the write path needs besides the database.
pub(crate) struct Context<'a> {
    pub server_id: &'a str,
    pub keys: &'a ServerKeys,
    pub now: SystemTime,
    /// The effective, operator-tunable limits (server-runtime: Configuration).
    pub limits: crate::runtime::config::Limits,
}

/// The result for one client envelope.
#[derive(Debug)]
pub(crate) enum Outcome {
    /// Durably stored (or an identical retry of a stored one).
    Accepted {
        server_seq: u64,
        accepted_at: SystemTime,
        /// Devices that got new entries, to wake or ping.
        notify: Vec<DeviceId>,
        /// Notices to send to connected devices (direct-messages: DM members
        /// kept complete, after a main-group commit adds a device back).
        notices: Vec<(DeviceId, pb::ServerFrame)>,
        /// Devices whose own user's access just changed (removed from the
        /// conversation, or added to a new named group): they get a
        /// content-free wake push although the entry is control traffic
        /// (message-delivery: Content-free wake for an own-access admin-event).
        own_access: Vec<DeviceId>,
    },
    /// Refused; nothing stored.
    Rejected {
        reason: RejectionReason,
        /// The group's epoch, with `STALE_EPOCH`.
        current_epoch: u64,
    },
}

fn reject(reason: RejectionReason) -> Outcome {
    Outcome::Rejected {
        reason,
        current_epoch: 0,
    }
}

/// The checks that need no database: `msg_id`, kind, size, and where a
/// Welcome bundle may appear.
pub(crate) fn precheck(envelope: &pb::Envelope) -> Result<Kind, RejectionReason> {
    if envelope.msg_id.len() != MSG_ID_LEN {
        return Err(RejectionReason::Malformed);
    }
    let kind = Kind::try_from(envelope.kind).map_err(|_| RejectionReason::Malformed)?;
    match kind {
        Kind::Application => {
            if envelope.ciphertext.len() > MAX_APPLICATION_CIPHERTEXT_BYTES {
                return Err(RejectionReason::TooLarge);
            }
        }
        Kind::Commit | Kind::Proposal => {
            if envelope.encoded_len() > MAX_HANDSHAKE_ENVELOPE_BYTES {
                return Err(RejectionReason::TooLarge);
            }
        }
        // Only the server sends Welcomes (message-delivery: Framing matches the
        // envelope).
        Kind::Welcome | Kind::Unspecified => return Err(RejectionReason::Malformed),
    }
    if kind != Kind::Commit && envelope.welcome.is_some() {
        return Err(RejectionReason::Malformed);
    }
    Ok(kind)
}

/// message-delivery: Framing matches the envelope. The MLS message parses,
/// has the wire format of `kind` (`PrivateMessage` for `APPLICATION`,
/// `PublicMessage` for handshakes), and its group id, epoch and content type
/// are the envelope's.
fn framing_matches(envelope: &pb::Envelope, kind: Kind) -> bool {
    let Ok(message) = MlsMessageIn::tls_deserialize_exact(envelope.ciphertext.as_slice()) else {
        return false;
    };
    let Ok(message) = message.try_into_protocol_message() else {
        return false;
    };
    let (wire_ok, content_type) = match kind {
        Kind::Application => (
            matches!(message, ProtocolMessage::PrivateMessage(_)),
            ContentType::Application,
        ),
        Kind::Commit => (
            matches!(message, ProtocolMessage::PublicMessage(_)),
            ContentType::Commit,
        ),
        Kind::Proposal => (
            matches!(message, ProtocolMessage::PublicMessage(_)),
            ContentType::Proposal,
        ),
        Kind::Welcome | Kind::Unspecified => return false,
    };
    wire_ok
        && message.content_type() == content_type
        && message.group_id().as_slice() == envelope.group_id.as_slice()
        && message.epoch().as_u64() == envelope.epoch
}

/// message-delivery: Welcome travels with its commit. A commit that adds
/// devices carries a bundle whose Welcome parses and whose recipients are
/// exactly the added devices; a commit that adds none carries no bundle.
fn welcome_matches(
    bundle: Option<&pb::WelcomeBundle>,
    accepted: &AcceptedCommit,
    server_id: &str,
) -> bool {
    let Some(bundle) = bundle else {
        return accepted.added.is_empty();
    };
    if accepted.added.is_empty() {
        return false;
    }
    let is_welcome = MlsMessageIn::tls_deserialize_exact(bundle.welcome.as_slice())
        .is_ok_and(|m| matches!(m.extract(), MlsMessageBodyIn::Welcome(_)));
    if !is_welcome || bundle.recipients.len() != accepted.added.len() {
        return false;
    }
    accepted.added.iter().all(|added| {
        bundle.recipients.iter().any(|r| {
            r.device_id.as_slice() == added.device_id.as_slice()
                && r.user_id
                    .as_ref()
                    .is_some_and(|u| u.username == added.username && u.server_id == server_id)
        })
    })
}

fn sha256(bytes: &[u8]) -> Vec<u8> {
    Sha256::digest(bytes).to_vec()
}

/// Checks and, if accepted, stores one envelope from `caller` (after
/// [`precheck`]). Runs inside the caller's transaction; commit it only on
/// [`Outcome::Accepted`] (a rejection writes nothing anyway).
pub(crate) fn submit(
    conn: &Transaction<'_>,
    ctx: &Context<'_>,
    caller: &Caller,
    envelope: &pb::Envelope,
    kind: Kind,
) -> Result<Outcome, StoreError> {
    // message-delivery: Idempotent sends.
    let ciphertext_sha256 = sha256(&envelope.ciphertext);
    let welcome_sha256 = envelope
        .welcome
        .as_ref()
        .map(|bundle| sha256(&bundle.encode_to_vec()));
    if let Some(original) = log::dedup_lookup(conn, &caller.device_id, &envelope.msg_id)? {
        if original.ciphertext_sha256 == ciphertext_sha256
            && original.welcome_sha256 == welcome_sha256
        {
            return Ok(Outcome::Accepted {
                server_seq: original.server_seq,
                accepted_at: original.accepted_at,
                notify: Vec::new(),
                notices: Vec::new(),
                own_access: Vec::new(),
            });
        }
        return Ok(reject(RejectionReason::DuplicateMismatch));
    }

    let Some(row) = group::row(conn, &envelope.group_id)? else {
        return Ok(reject(RejectionReason::Malformed));
    };
    // direct-messages: DM closed on user removal.
    if row.kind.is_closed() {
        return Ok(reject(RejectionReason::GroupClosed));
    }
    let group_kind = row.kind.clone();
    let (group_id, epoch) = (row.group_id, row.epoch);
    if !framing_matches(envelope, kind) {
        return Ok(reject(RejectionReason::Malformed));
    }
    // message-delivery: Strict epoch rule.
    if envelope.epoch != epoch {
        return Ok(Outcome::Rejected {
            reason: RejectionReason::StaleEpoch,
            current_epoch: epoch,
        });
    }
    // Only active members send: a device removed for inactivity waits for
    // its re-add (group-membership: Inactive devices are kept out).
    let recipients: Vec<DeviceId> = inactivity::active_members(conn, &group_id)?
        .into_iter()
        .map(|(id, _)| id)
        .collect();
    if !recipients.contains(&caller.device_id) {
        return Ok(reject(RejectionReason::PolicyViolation));
    }
    let recipients: Vec<DeviceId> = recipients
        .into_iter()
        .filter(|id| *id != caller.device_id)
        .collect();

    // message-deletion: Who may delete. An `APPLICATION` envelope that names a
    // delete target in `deletes` is authorised from the stream metadata before
    // anything is stored: an admin device in the lobby or a named group, or
    // (in a DM) the target's original sender, else `POLICY_VIOLATION` with
    // nothing stored. The server never reads the encrypted `Content`.
    if kind == Kind::Application
        && let Some(target) = &envelope.deletes
    {
        let authority = match group_kind {
            GroupKind::System | GroupKind::Named { .. } => DeleteAuthority::Group,
            GroupKind::Dm { .. } => DeleteAuthority::Dm,
        };
        let is_target_sender = target
            .target_sender
            .as_ref()
            .is_some_and(|sender| sender.device_id.as_slice() == caller.device_id.as_slice());
        if !may_delete(authority, caller.is_admin, is_target_sender) {
            return Ok(reject(RejectionReason::PolicyViolation));
        }
    }

    let mut group = None;
    let mut accepted_commit = None;
    match kind {
        Kind::Commit => {
            let mut state = GroupState::load(conn, &group_id)?
                .ok_or_else(|| StoreError::Inconsistent("group row without state".into()))?;
            let accepted =
                match state.check_commit(conn, caller, ctx.server_id, &envelope.ciphertext)? {
                    Ok(accepted) => accepted,
                    Err(rejection) => {
                        return Ok(Outcome::Rejected {
                            reason: rejection.reason(),
                            current_epoch: rejection.current_epoch().unwrap_or(0),
                        });
                    }
                };
            if !welcome_matches(envelope.welcome.as_ref(), &accepted, ctx.server_id) {
                return Ok(reject(RejectionReason::Malformed));
            }
            group = Some(state);
            accepted_commit = Some(accepted);
        }
        Kind::Proposal => {
            let mut state = GroupState::load(conn, &group_id)?
                .ok_or_else(|| StoreError::Inconsistent("group row without state".into()))?;
            if let Err(rejection) = state.check_proposal(conn, caller, &envelope.ciphertext)? {
                return Ok(Outcome::Rejected {
                    reason: rejection.reason(),
                    current_epoch: rejection.current_epoch().unwrap_or(0),
                });
            }
            group = Some(state);
        }
        Kind::Application | Kind::Welcome | Kind::Unspecified => {}
    }

    // Accepted: write everything in this transaction.
    let server_seq = log::next_seq(conn)?;
    let accepted_at = ctx.now;
    log::insert_envelope(
        conn,
        &NewEnvelope {
            server_seq,
            group_id: &group_id,
            msg_id: &envelope.msg_id,
            sender: Some(&caller.device_id),
            kind,
            epoch,
            ciphertext: &envelope.ciphertext,
            welcome: envelope.welcome.as_ref().map(|b| b.welcome.as_slice()),
            accepted_at,
        },
    )?;
    log::dedup_insert(
        conn,
        &caller.device_id,
        &envelope.msg_id,
        &DedupRow {
            server_seq,
            accepted_at,
            ciphertext_sha256,
            welcome_sha256,
        },
    )?;

    let mut notify = Vec::new();
    let mut notices = Vec::new();
    let mut own_access = Vec::new();
    match kind {
        Kind::Application => {
            // message-delivery: Per-device queues. A full queue never fails
            // the send; that device is marked inactive instead.
            let mut full = Vec::new();
            for recipient in &recipients {
                if log::application_queue_len(conn, recipient)? >= ctx.limits.queue_cap {
                    full.push(*recipient);
                } else {
                    log::insert_delivery(conn, recipient, server_seq, false)?;
                    notify.push(*recipient);
                }
            }
            for device_id in &full {
                notify.extend(mark_inactive(conn, ctx, device_id)?);
            }
            // message-deletion: Deletion retracts undelivered copies. After
            // queuing the authorised Delete, drop the target's still-queued
            // `APPLICATION` copies for every device that has not acked them, so
            // a device not yet delivered the original never gets its plaintext,
            // only the delete (design D4). The Delete itself was just queued
            // above, so the retract is always paired with a delivered
            // tombstone. The target need not still exist: an expired or
            // unknown target simply has nothing to retract.
            if let Some(target) = &envelope.deletes
                && let Some(sender) = &target.target_sender
                && let Some(target_sender) =
                    crate::store::records::device_id_from(&sender.device_id)
            {
                log::retract_undelivered(conn, &target_sender, &target.target_msg_id)?;
            }
        }
        Kind::Proposal => {
            for recipient in &recipients {
                log::insert_delivery(conn, recipient, server_seq, false)?;
                notify.push(*recipient);
            }
            if let Some(state) = &group {
                state.save(conn)?;
            }
        }
        Kind::Commit => {
            // The members of the epoch the commit was sent in get the commit;
            // the devices it adds get its Welcome instead (message-delivery:
            // Commit log; Welcome travels with its commit).
            for recipient in &recipients {
                log::insert_delivery(conn, recipient, server_seq, false)?;
                notify.push(*recipient);
            }
            if let (Some(mut state), Some(accepted)) = (group, accepted_commit) {
                own_access.extend(own_access_changes(
                    conn,
                    state.kind(),
                    &group_id,
                    caller,
                    &accepted,
                )?);
                for added in &accepted.added {
                    log::insert_delivery(conn, &added.device_id, server_seq, true)?;
                    // Back in the main group means active again; its DMs
                    // follow (direct-messages: Inactivity across groups).
                    if *state.kind() == GroupKind::System {
                        inactivity::clear(conn, &added.device_id, ctx.now)?;
                    }
                    notify.push(added.device_id);
                }
                state.save(conn)?;
                // The new epoch needs the server's pending inactivity Removes
                // again (group-membership: Inactive devices are kept out).
                notify.extend(reissue_removes(conn, ctx, &mut state)?);
                state.save(conn)?;
                if *state.kind() == GroupKind::System {
                    for added in &accepted.added {
                        notices.extend(joins::dm_notices_about(
                            conn,
                            &added.device_id,
                            ctx.server_id,
                        )?);
                    }
                }
            }
        }
        Kind::Welcome | Kind::Unspecified => {}
    }
    notify.sort_unstable();
    notify.dedup();
    Ok(Outcome::Accepted {
        server_seq,
        accepted_at,
        notify,
        notices,
        own_access,
    })
}

/// message-delivery: Content-free wake for an own-access admin-event. The devices
/// of a user whose access to this conversation an accepted commit changes for
/// good: every device of a user the commit removes entirely, and the devices of
/// a user the commit adds to a *named* group where they had none. Not a user's
/// other device being linked or unlinked, not the committer's own leave, not the
/// server's inactivity Removes, and not the lobby or a DM (routine churn). Read
/// before the commit is saved, so `mls_members` still holds the old members.
fn own_access_changes(
    conn: &Transaction<'_>,
    kind: &GroupKind,
    group_id: &[u8],
    caller: &Caller,
    accepted: &AcceptedCommit,
) -> Result<Vec<DeviceId>, StoreError> {
    let before = GroupState::members(conn, group_id)?;
    let in_before = |user: &str| before.iter().any(|m| m.username == user);
    let mut wake = Vec::new();
    for removed in &accepted.removed {
        let stays = before.iter().any(|m| {
            m.username == removed.username
                && !accepted.removed.iter().any(|r| r.device_id == m.device_id)
        });
        if removed.username != caller.username
            && !stays
            && !inactivity::is_inactive(conn, &removed.device_id)?
        {
            wake.push(removed.device_id);
        }
    }
    if matches!(kind, GroupKind::Named { .. }) {
        for added in &accepted.added {
            if !in_before(&added.username) {
                wake.push(added.device_id);
            }
        }
    }
    Ok(wake)
}

/// group-membership: Inactive devices are kept out. Marks `device_id`
/// inactive: deletes everything queued for it, in every group, and issues
/// the server's external Remove proposal for its leaf in every group it is in
/// (direct-messages: Inactivity across groups). Saves each group. Returns
/// the devices that got a proposal.
pub(crate) fn mark_inactive(
    conn: &Transaction<'_>,
    ctx: &Context<'_>,
    device_id: &DeviceId,
) -> Result<Vec<DeviceId>, StoreError> {
    if inactivity::is_inactive(conn, device_id)? {
        return Ok(Vec::new());
    }
    let oldest = log::delete_all_deliveries(conn, device_id)?;
    inactivity::mark(conn, device_id, ctx.now, oldest.unwrap_or(ctx.now))?;
    tracing::info!(
        device = %crate::runtime::redact(device_id),
        "marked a device inactive"
    );
    let mut notify = Vec::new();
    for row in group::groups_of(conn, device_id)? {
        let mut state = GroupState::load(conn, &row.group_id)?
            .ok_or_else(|| StoreError::Inconsistent("group row without state".into()))?;
        notify.extend(propose_remove(conn, ctx, &mut state, device_id)?);
        state.save(conn)?;
    }
    Ok(notify)
}

/// Issues (or reissues) the server's external Remove of `device_id`'s leaf in the
/// current epoch of `state`'s group as a `PROPOSAL` envelope to every active
/// member of that group.
fn propose_remove(
    conn: &Transaction<'_>,
    ctx: &Context<'_>,
    state: &mut GroupState,
    device_id: &DeviceId,
) -> Result<Vec<DeviceId>, StoreError> {
    let Some(leaf) = state.leaf_of(conn, device_id)? else {
        return Ok(Vec::new());
    };
    let proposal = state.issue_external_remove(leaf, ctx.keys)?;
    let mut msg_id = [0u8; MSG_ID_LEN];
    getrandom::fill(&mut msg_id)
        .map_err(|e| StoreError::Inconsistent(format!("no randomness for a msg_id: {e}")))?;
    let server_seq = log::next_seq(conn)?;
    log::insert_envelope(
        conn,
        &NewEnvelope {
            server_seq,
            group_id: state.group_id(),
            msg_id: &msg_id,
            sender: None,
            kind: Kind::Proposal,
            epoch: state.epoch(),
            ciphertext: &proposal,
            welcome: None,
            accepted_at: ctx.now,
        },
    )?;
    let mut notify = Vec::new();
    for (member, _) in inactivity::active_members(conn, state.group_id())? {
        log::insert_delivery(conn, &member, server_seq, false)?;
        notify.push(member);
    }
    inactivity::set_proposed(conn, device_id, state.group_id(), state.epoch())?;
    tracing::info!(
        leaf,
        epoch = state.epoch(),
        server_seq,
        "issued an external Remove proposal"
    );
    Ok(notify)
}

/// Reissues the server's Remove for every inactive device still in the tree that
/// has none for the current epoch. Persist `state` afterwards.
pub(crate) fn reissue_removes(
    conn: &Transaction<'_>,
    ctx: &Context<'_>,
    state: &mut GroupState,
) -> Result<Vec<DeviceId>, StoreError> {
    let mut notify = Vec::new();
    let group_id = state.group_id().to_vec();
    for device_id in inactivity::needing_proposal(conn, &group_id, state.epoch())? {
        notify.extend(propose_remove(conn, ctx, state, &device_id)?);
    }
    Ok(notify)
}
