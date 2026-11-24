//! The engine's jobs: everything this device starts on its own or for the
//! UI that needs the connection and, for most, an own commit (client-core
//! tasks 5.1, 5.3, 5.4).
//!
//! A job is prepared when the stream is quiet and nothing is in flight. A
//! commit job stages its commit and hands it to [`Engine::send_commit`]; on
//! `STALE_EPOCH` it is prepared again from scratch once the device caught up,
//! so it re-checks whether it is still needed (a re-add someone else already
//! did, a rotation another commit made unnecessary).

use openmls::prelude::tls_codec::Serialize as _;
use openmls::prelude::{KeyPackage, LeafNodeIndex};
use openmls_traits::OpenMlsProvider as _;

use super::{Engine, EngineError, Reply};
use crate::client::ClientError;
use crate::events::{DeviceId, ReaddOutcome, UserId};
use crate::identity::certificate::credential_with_key;
use crate::membership::{
    self, CommitPlan, StagedOwnCommit, check_fetched, device_ref, leaf_fingerprint, leaves_of_user,
    pending_removals,
};
use crate::mls::group as mls_group;
use crate::mls::verify::leaf_of;
use crate::net::NetError;
use crate::pb;

/// Something the engine does when it gets to it.
pub(crate) enum Job {
    /// Admin: create the group (group-membership: Group creation).
    CreateGroup { reply: Option<Reply> },
    /// Admin: add all of a user's devices with one commit and its Welcome.
    AddUser { user: UserId, reply: Option<Reply> },
    /// Admin: commit Removes of all of a user's devices, then
    /// `Admin.RemoveUser` (group-membership: Removing a user).
    RemoveUser { user: UserId, reply: Option<Reply> },
    /// Remove a device of this user from the group (after revoking it).
    RemoveDevice {
        device: DeviceId,
        reply: Option<Reply>,
    },
    /// group-membership: Automatic re-add. `seen` is the encryption key of
    /// the device's leaf when the notice came (`None`: it had none).
    Readd {
        user: UserId,
        device: DeviceId,
        seen: Option<Vec<u8>>,
    },
    /// group-membership: Automatic add of new users. Admin: add a new user's
    /// device the server announced with a `JoinNotice`.
    AddNew { user: UserId, device: DeviceId },
    /// device-identity: Device linking. Add a device this device linked,
    /// once it has uploaded KeyPackages.
    AddLinked { device: DeviceId },
    /// Admin: commit the server's pending inactivity Removes.
    CommitServerRemoves,
    /// group-membership: Own leaf rotation.
    Rotate,
    /// key-directory: upload KeyPackages. `Some(n)`: top up one-time
    /// KeyPackages from `n` (signup, linking, a low notice); `None`: only
    /// replace the last-resort one (weekly refresh). Every upload carries a
    /// new last-resort KeyPackage.
    UploadKeyPackages { remaining: Option<u32> },
    /// group-membership: Re-add after an unusable Welcome. Ask the server to be
    /// added again, with fresh KeyPackages: after refusing the Welcome at
    /// `refused_seq`, or (`0`) at start with no group.
    RequestReadd { refused_seq: u64, reason: String },
    /// direct-messages: Opening a DM with `peer`.
    OpenDm { peer: UserId, reply: Option<Reply> },
    /// direct-messages: DM members kept complete. Add `device` of `user` to
    /// the DM `group_id`, with the server's pending Remove of its old leaf.
    AddToDm {
        group_id: Vec<u8>,
        user: UserId,
        device: DeviceId,
    },
    /// direct-messages: Inactivity across groups. Commit the server's pending
    /// Removes in the DM `group_id`.
    CommitServerRemovesIn { group_id: Vec<u8> },
    /// group-membership: Own leaf rotation, in the DM `group_id`.
    RotateIn { group_id: Vec<u8> },
    /// multi-group: Named group creation. Admin: the named group `group_id` at
    /// epoch 0, then `Admin.CreateNamedGroup`; the name and the `members` are
    /// queued after it.
    CreateNamedGroup {
        group_id: Vec<u8>,
        name: Option<String>,
        members: Vec<UserId>,
        reply: Option<Reply>,
    },
    /// multi-group: Named group membership. Admin: add all of `user`'s
    /// version 3 devices to the named group `group_id`.
    AddToGroup {
        group_id: Vec<u8>,
        user: UserId,
        reply: Option<Reply>,
    },
    /// multi-group: Named group membership. Admin: remove all of `user`'s
    /// devices from the named group `group_id`; `after` runs next (removing a
    /// user from every group, then `Admin.RemoveUser`).
    RemoveFromGroup {
        group_id: Vec<u8>,
        user: UserId,
        reply: Option<Reply>,
        after: Option<Box<Job>>,
    },
    /// multi-group: Closing a named group. Admin: `Admin.CloseGroup`.
    CloseGroup {
        group_id: Vec<u8>,
        reply: Option<Reply>,
    },
    /// message-deletion: Remove a closed DM. `Dms.RemoveClosedDm`, then the
    /// local delete.
    RemoveClosedDm {
        group_id: Vec<u8>,
        reply: Option<Reply>,
    },
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn unhex(text: &str) -> Option<Vec<u8>> {
    if !text.len().is_multiple_of(2) {
        return None;
    }
    (0..text.len())
        .step_by(2)
        .map(|i| {
            text.get(i..i + 2)
                .and_then(|b| u8::from_str_radix(b, 16).ok())
        })
        .collect()
}

fn parse_user(text: &str) -> Option<UserId> {
    let (username, server_id) = text.split_once('@')?;
    Some(UserId::new(username, server_id))
}

fn parse_device(text: &str) -> Option<DeviceId> {
    DeviceId::try_from(unhex(text)?.as_slice()).ok()
}

/// What preparing a job produced.
enum Prepared {
    /// A staged commit and the devices its Welcome is for.
    Commit(StagedOwnCommit, Vec<pb::DeviceRef>),
    /// Nothing to send; the job is done.
    Nothing,
}

impl Job {
    /// Jobs that are the same whoever queued them, so one is enough.
    pub(crate) fn is_idempotent(&self) -> bool {
        matches!(
            self,
            Job::Readd { .. }
                | Job::AddNew { .. }
                | Job::AddLinked { .. }
                | Job::CommitServerRemoves
                | Job::Rotate
                | Job::UploadKeyPackages { .. }
                | Job::AddToDm { .. }
                | Job::CommitServerRemovesIn { .. }
                | Job::RotateIn { .. }
        )
    }

    /// The DM a job commits in; `None` for the main group (or no commit).
    pub(crate) fn dm_group(&self) -> Option<&[u8]> {
        match self {
            Job::AddToDm { group_id, .. }
            | Job::AddToGroup { group_id, .. }
            | Job::RemoveFromGroup { group_id, .. }
            | Job::CommitServerRemovesIn { group_id }
            | Job::RotateIn { group_id } => Some(group_id),
            _ => None,
        }
    }

    pub(crate) fn same_as(&self, other: &Job) -> bool {
        match (self, other) {
            (Job::Readd { device: a, .. }, Job::Readd { device: b, .. })
            | (Job::AddNew { device: a, .. }, Job::AddNew { device: b, .. }) => a == b,
            (Job::AddLinked { device: a }, Job::AddLinked { device: b }) => a == b,
            (Job::CommitServerRemoves, Job::CommitServerRemoves)
            | (Job::Rotate, Job::Rotate)
            | (Job::UploadKeyPackages { .. }, Job::UploadKeyPackages { .. }) => true,
            (
                Job::AddToDm {
                    group_id: a,
                    device: x,
                    ..
                },
                Job::AddToDm {
                    group_id: b,
                    device: y,
                    ..
                },
            ) => a == b && x == y,
            (
                Job::CommitServerRemovesIn { group_id: a },
                Job::CommitServerRemovesIn { group_id: b },
            )
            | (Job::RotateIn { group_id: a }, Job::RotateIn { group_id: b }) => a == b,
            _ => false,
        }
    }

    /// What the stored commit was for, so a restart can finish it.
    pub(crate) fn purpose(&self) -> String {
        match self {
            Job::CreateGroup { .. } => "create_group".into(),
            Job::AddUser { user, .. } => format!("add_user|{user}"),
            Job::RemoveUser { user, .. } => format!("remove_user|{user}"),
            Job::RemoveDevice { device, .. } => format!("remove_device|{}", hex(device)),
            Job::Readd { user, device, seen } => format!(
                "readd|{user}|{}|{}",
                hex(device),
                seen.as_deref().map(hex).unwrap_or_default()
            ),
            Job::AddNew { user, device } => format!("add_new|{user}|{}", hex(device)),
            Job::AddLinked { device } => format!("add_linked|{}", hex(device)),
            Job::CommitServerRemoves => "pi_removes".into(),
            Job::Rotate => "rotate".into(),
            Job::UploadKeyPackages { .. } => "upload_key_packages".into(),
            Job::RequestReadd { .. } => "request_readd".into(),
            Job::OpenDm { peer, .. } => format!("open_dm|{peer}"),
            Job::AddToDm {
                group_id,
                user,
                device,
            } => format!("dm_add|{}|{user}|{}", hex(group_id), hex(device)),
            Job::CommitServerRemovesIn { group_id } => {
                format!("dm_server_removes|{}", hex(group_id))
            }
            Job::RotateIn { group_id } => format!("dm_rotate|{}", hex(group_id)),
            Job::CreateNamedGroup { group_id, .. } => format!("ng_create|{}", hex(group_id)),
            Job::AddToGroup { group_id, user, .. } => {
                format!("ng_add|{}|{user}", hex(group_id))
            }
            Job::RemoveFromGroup { group_id, user, .. } => {
                format!("ng_remove|{}|{user}", hex(group_id))
            }
            Job::CloseGroup { group_id, .. } => format!("ng_close|{}", hex(group_id)),
            Job::RemoveClosedDm { group_id, .. } => format!("dm_remove|{}", hex(group_id)),
        }
    }

    /// The job behind a stored commit's purpose (after a crash; nobody waits
    /// for its reply any more).
    pub(crate) fn recovered(purpose: &str) -> Job {
        let parts: Vec<&str> = purpose.split('|').collect();
        let job = match parts.as_slice() {
            ["add_user", user] => parse_user(user).map(|user| Job::AddUser { user, reply: None }),
            ["remove_user", user] => {
                parse_user(user).map(|user| Job::RemoveUser { user, reply: None })
            }
            ["remove_device", device] => parse_device(device).map(|device| Job::RemoveDevice {
                device,
                reply: None,
            }),
            ["readd", user, device, seen] => match (parse_user(user), parse_device(device)) {
                (Some(user), Some(device)) => Some(Job::Readd {
                    user,
                    device,
                    seen: (!seen.is_empty()).then(|| unhex(seen)).flatten(),
                }),
                _ => None,
            },
            ["add_new", user, device] => match (parse_user(user), parse_device(device)) {
                (Some(user), Some(device)) => Some(Job::AddNew { user, device }),
                _ => None,
            },
            ["add_linked", device] => parse_device(device).map(|device| Job::AddLinked { device }),
            ["pi_removes"] => Some(Job::CommitServerRemoves),
            ["dm_add", group, user, device] => {
                match (unhex(group), parse_user(user), parse_device(device)) {
                    (Some(group_id), Some(user), Some(device)) => Some(Job::AddToDm {
                        group_id,
                        user,
                        device,
                    }),
                    _ => None,
                }
            }
            ["dm_server_removes", group] => {
                unhex(group).map(|group_id| Job::CommitServerRemovesIn { group_id })
            }
            ["dm_rotate", group] => unhex(group).map(|group_id| Job::RotateIn { group_id }),
            ["ng_add", group, user] => match (unhex(group), parse_user(user)) {
                (Some(group_id), Some(user)) => Some(Job::AddToGroup {
                    group_id,
                    user,
                    reply: None,
                }),
                _ => None,
            },
            ["ng_remove", group, user] => match (unhex(group), parse_user(user)) {
                (Some(group_id), Some(user)) => Some(Job::RemoveFromGroup {
                    group_id,
                    user,
                    reply: None,
                    after: None,
                }),
                _ => None,
            },
            _ => None,
        };
        job.unwrap_or(Job::Rotate)
    }

    fn take_reply(&mut self) -> Option<Reply> {
        match self {
            Job::CreateGroup { reply }
            | Job::AddUser { reply, .. }
            | Job::RemoveUser { reply, .. }
            | Job::RemoveDevice { reply, .. }
            | Job::OpenDm { reply, .. }
            | Job::CreateNamedGroup { reply, .. }
            | Job::AddToGroup { reply, .. }
            | Job::RemoveFromGroup { reply, .. }
            | Job::CloseGroup { reply, .. }
            | Job::RemoveClosedDm { reply, .. } => reply.take(),
            _ => None,
        }
    }

    /// Ends the job with `error`.
    pub(crate) fn fail(mut self, error: ClientError) {
        // A removal chained to this one cannot run now: it fails too.
        if let Job::RemoveFromGroup { after, reply, .. } = &mut self
            && reply.is_none()
            && let Some(after) = after.take()
        {
            after.fail(error);
            return;
        }
        match self.take_reply() {
            Some(reply) => {
                let _ = reply.send(Err(error));
            }
            None => tracing::warn!(purpose = %self.purpose(), %error, "job failed"),
        }
    }

    fn succeed(mut self) {
        // A chained job still has to run: the caller queues it (see `finish`).
        if let Some(reply) = self.take_reply() {
            let _ = reply.send(Ok(()));
        }
    }
}

impl Engine {
    pub(super) async fn run_job(&mut self, job: Job) -> Result<(), EngineError> {
        match job {
            Job::CreateGroup { .. } => {
                let result = self.create_group().await;
                match result {
                    Ok(()) => job.succeed(),
                    Err(error) => job.fail(error),
                }
                Ok(())
            }
            Job::OpenDm { ref peer, .. } => {
                let peer = peer.clone();
                let result = self.open_dm(&peer).await;
                match result {
                    Ok(()) => job.succeed(),
                    Err(error) => job.fail(error),
                }
                Ok(())
            }
            Job::CreateNamedGroup { .. } => {
                let result = self.create_named_group(&job).await;
                match result {
                    Ok(()) => job.succeed(),
                    Err(error) => job.fail(error),
                }
                Ok(())
            }
            Job::CloseGroup { ref group_id, .. } => {
                let group_id = group_id.clone();
                let result = self.close_named_group(&group_id).await;
                match result {
                    Ok(()) => job.succeed(),
                    Err(error) => job.fail(error),
                }
                Ok(())
            }
            Job::RemoveClosedDm { ref group_id, .. } => {
                let group_id = group_id.clone();
                let result = self.remove_closed_dm(&group_id).await;
                match result {
                    Ok(()) => job.succeed(),
                    Err(error) => job.fail(error),
                }
                Ok(())
            }
            Job::RequestReadd {
                refused_seq,
                reason,
            } => {
                if let Err(error) = self.request_readd(refused_seq, &reason).await {
                    tracing::warn!(%error, "asking to be added again failed");
                    if matches!(
                        error,
                        ClientError::Net(NetError::Connect(_)) | ClientError::Offline
                    ) {
                        self.push_job(Job::RequestReadd {
                            refused_seq,
                            reason,
                        });
                    }
                }
                Ok(())
            }
            Job::UploadKeyPackages { remaining } => {
                if let Err(error) = self.upload_key_packages(remaining).await {
                    tracing::warn!(%error, "KeyPackage upload failed");
                    if matches!(
                        error,
                        ClientError::Net(NetError::Connect(_)) | ClientError::Offline
                    ) {
                        self.push_job(Job::UploadKeyPackages { remaining });
                    }
                }
                Ok(())
            }
            job => {
                let prepared = self.prepare(&job).await;
                match prepared {
                    Ok(Prepared::Commit(staged, recipients)) => {
                        self.send_commit(job, staged, recipients).await
                    }
                    Ok(Prepared::Nothing) => {
                        self.finish(job).await;
                        Ok(())
                    }
                    // Not ready yet (a linked device without KeyPackages):
                    // the next poll queues it again.
                    Err(ClientError::NotYet) => Ok(()),
                    Err(ClientError::Offline) => {
                        // Try again once reconnected.
                        self.jobs.push_front(job);
                        Ok(())
                    }
                    Err(error) => {
                        job.fail(error);
                        Ok(())
                    }
                }
            }
        }
    }

    /// What to do after the server accepted a job's commit (or the job needed
    /// none).
    pub(super) async fn finish(&mut self, mut job: Job) {
        match job {
            Job::RemoveUser {
                ref user,
                ref mut reply,
            } => {
                // multi-group: Removing a user. Every named group the user is
                // in loses their devices first, one commit at a time.
                if let Some(group_id) = self.named_group_with(user) {
                    let user = user.clone();
                    let reply = reply.take();
                    self.jobs.push_front(Job::RemoveFromGroup {
                        group_id,
                        user: user.clone(),
                        reply: None,
                        after: Some(Box::new(Job::RemoveUser { user, reply })),
                    });
                    return;
                }
                let result = self.admin_remove_user(user).await;
                match result {
                    Ok(()) => job.succeed(),
                    Err(error) => job.fail(error),
                }
            }
            Job::AddLinked { device } => {
                if let Err(error) = self.store().remove_pending_link(&device) {
                    tracing::warn!(%error, "cannot clear a pending link");
                }
                // multi-group: Named group membership. A member adds their own
                // new device to the named groups they are in too.
                let named: Vec<Vec<u8>> = self
                    .dms
                    .iter()
                    .filter(|(_, group)| mls_group::is_named(group.extensions()))
                    .map(|(group_id, _)| group_id.clone())
                    .collect();
                for group_id in named {
                    self.push_job(Job::AddToDm {
                        group_id,
                        user: self.user(),
                        device,
                    });
                }
            }
            Job::RemoveFromGroup {
                ref mut after,
                ref mut reply,
                ..
            } => {
                let next = after.take();
                if let Some(reply) = reply.take() {
                    let _ = reply.send(Ok(()));
                }
                if let Some(next) = next {
                    self.jobs.push_front(*next);
                }
            }
            job => job.succeed(),
        }
    }

    async fn admin_remove_user(&self, user: &UserId) -> Result<(), ClientError> {
        let session = self.session().map_err(ClientError::from)?;
        session
            .conn()
            .admin()
            .remove_user(session.request(pb::RemoveUserRequest {
                user_id: Some(user.to_pb()),
            })?)
            .await
            .map_err(NetError::from)?;
        tracing::info!(user = %user, "removed a user");
        Ok(())
    }

    /// Stages a commit in the main group in a store transaction; reloads
    /// the group if it fails.
    fn stage(&mut self, plan: CommitPlan) -> Result<StagedOwnCommit, ClientError> {
        let group_id = self.main_id().ok_or(ClientError::NoGroup)?;
        self.stage_in(&group_id, plan)
    }

    /// [`Engine::stage`] in the group `group_id` (change `dms`).
    fn stage_in(
        &mut self,
        group_id: &[u8],
        plan: CommitPlan,
    ) -> Result<StagedOwnCommit, ClientError> {
        let store = std::sync::Arc::clone(&self.shared.store);
        let signer = self.keys.mls_signer();
        let Some(group) = super::group_in(&mut self.group, &mut self.dms, group_id) else {
            return Err(ClientError::NoGroup);
        };
        let staged = store.transaction(|| -> Result<StagedOwnCommit, EngineError> {
            Ok(membership::stage(group, store.mls(), &signer, plan)?)
        });
        match staged {
            Ok(staged) => Ok(staged),
            Err(error) => {
                self.reload_group_in(group_id).map_err(ClientError::from)?;
                Err(error.into())
            }
        }
    }

    /// `Keys.FetchKeyPackages` for `user`, optionally only `devices`.
    async fn fetch(
        &self,
        user: &UserId,
        devices: &[DeviceId],
    ) -> Result<Option<pb::FetchKeyPackagesResponse>, ClientError> {
        self.fetch_in(user, devices, &[]).await
    }

    /// [`Engine::fetch`] for the DM `group_id` (key-directory: KeyPackage
    /// fetch, change `dms`): only KeyPackages that list `0xF0A2`. Empty
    /// `group_id`: for the main group.
    async fn fetch_in(
        &self,
        user: &UserId,
        devices: &[DeviceId],
        group_id: &[u8],
    ) -> Result<Option<pb::FetchKeyPackagesResponse>, ClientError> {
        let session = self.session().map_err(ClientError::from)?;
        let response = session
            .conn()
            .keys()
            .fetch_key_packages(session.request(pb::FetchKeyPackagesRequest {
                user_id: Some(user.to_pb()),
                device_ids: devices.iter().map(|d| d.to_vec()).collect(),
                group_id: group_id.to_vec(),
            })?)
            .await;
        match response {
            Ok(response) => Ok(Some(response.into_inner())),
            Err(status) if status.code() == tonic::Code::NotFound => Ok(None),
            Err(status) => Err(NetError::from(status).into()),
        }
    }

    /// Checks fetched KeyPackages of `user` against the identity key the
    /// fetch returned, after pinning it (key-directory: Clients verify the
    /// identity chain). Devices already in the group are left out.
    fn checked_key_packages(
        &self,
        user: &UserId,
        response: &pb::FetchKeyPackagesResponse,
    ) -> Result<Vec<(KeyPackage, pb::DeviceRef)>, ClientError> {
        self.check_pin(user, &response.identity_public_key)?;
        let group = self.group.as_ref().ok_or(ClientError::NoGroup)?;
        let mut checked = Vec::new();
        for entry in &response.devices {
            let (key_package, cert) = check_fetched(
                self.store().mls(),
                &response.identity_public_key,
                user,
                entry,
            )
            .map_err(|problem| ClientError::Certificate(problem.to_string()))?;
            if leaf_of(group, &cert.device_id).is_some() {
                continue;
            }
            checked.push((key_package, device_ref(user, &cert.device_id)));
        }
        Ok(checked)
    }

    async fn prepare(&mut self, job: &Job) -> Result<Prepared, ClientError> {
        match job {
            Job::AddUser { user, .. } => {
                self.require_admin()?;
                self.usable_group()?;
                let Some(response) = self.fetch(user, &[]).await? else {
                    return Err(ClientError::UnknownUser(user.clone()));
                };
                let checked = self.checked_key_packages(user, &response)?;
                if checked.is_empty() {
                    let all_in = !response.devices.is_empty();
                    return if all_in {
                        Ok(Prepared::Nothing)
                    } else {
                        Err(ClientError::NoKeyPackages(user.clone()))
                    };
                }
                let (adds, recipients): (Vec<_>, Vec<_>) = checked.into_iter().unzip();
                let staged = self.stage(CommitPlan {
                    adds,
                    ..Default::default()
                })?;
                Ok(Prepared::Commit(staged, recipients))
            }
            Job::RemoveUser { user, .. } => {
                self.require_admin()?;
                self.usable_group()?;
                let group = self.group.as_ref().ok_or(ClientError::NoGroup)?;
                let pending = pending_removals(group);
                let removes: Vec<LeafNodeIndex> = leaves_of_user(group, user)
                    .into_iter()
                    .map(|(leaf, _)| leaf)
                    .filter(|leaf| !pending.contains(leaf))
                    .collect();
                if removes.is_empty()
                    && !membership::has_pending_server_removes(group, self.server_index())
                {
                    return Ok(Prepared::Nothing);
                }
                let staged = self.stage(CommitPlan {
                    removes,
                    ..Default::default()
                })?;
                Ok(Prepared::Commit(staged, Vec::new()))
            }
            Job::RemoveDevice { device, .. } => {
                self.usable_group()?;
                let group = self.group.as_ref().ok_or(ClientError::NoGroup)?;
                let Some(leaf) = leaf_of(group, device) else {
                    return Ok(Prepared::Nothing);
                };
                if pending_removals(group).contains(&LeafNodeIndex::new(leaf)) {
                    return Ok(Prepared::Nothing);
                }
                let staged = self.stage(CommitPlan {
                    removes: vec![LeafNodeIndex::new(leaf)],
                    ..Default::default()
                })?;
                Ok(Prepared::Commit(staged, Vec::new()))
            }
            Job::Readd { user, device, seen } => {
                self.usable_group()?;
                let group = self.group.as_ref().ok_or(ClientError::NoGroup)?;
                let current = leaf_fingerprint(group, device);
                if let Some((_, key)) = &current
                    && Some(key) != seen.as_ref()
                {
                    // Someone else re-added it already.
                    tracing::info!("the device is already back; nothing to re-add");
                    return Ok(Prepared::Nothing);
                }
                let Some(response) = self.fetch(user, &[*device]).await? else {
                    return Ok(Prepared::Nothing);
                };
                let mut checked = self.checked_key_packages(user, &response)?;
                // The old leaf is still in the tree, so `checked_key_packages`
                // left it out; check it against the identity key directly.
                if checked.is_empty() && current.is_some() {
                    for entry in &response.devices {
                        let (key_package, cert) = check_fetched(
                            self.store().mls(),
                            &response.identity_public_key,
                            user,
                            entry,
                        )
                        .map_err(|problem| ClientError::Certificate(problem.to_string()))?;
                        if cert.device_id == *device {
                            checked.push((key_package, device_ref(user, device)));
                        }
                    }
                }
                let Some((key_package, recipient)) = checked.into_iter().next() else {
                    return Ok(Prepared::Nothing);
                };
                let group = self.group.as_ref().ok_or(ClientError::NoGroup)?;
                let pending = pending_removals(group);
                let removes = current
                    .map(|(leaf, _)| leaf)
                    .filter(|leaf| !pending.contains(leaf))
                    .into_iter()
                    .collect();
                let staged = self.stage(CommitPlan {
                    adds: vec![key_package],
                    removes,
                    force_path: false,
                })?;
                Ok(Prepared::Commit(staged, vec![recipient]))
            }
            Job::AddNew { user, device } => {
                self.require_admin()?;
                self.usable_group()?;
                let group = self.group.as_ref().ok_or(ClientError::NoGroup)?;
                if leaf_of(group, device).is_some() {
                    // Another admin device added it first (it lost the race
                    // with STALE_EPOCH and caught up), or it is in already.
                    tracing::info!("the new device is already a member; nothing to add");
                    return Ok(Prepared::Nothing);
                }
                let Some(response) = self.fetch(user, &[*device]).await? else {
                    return Ok(Prepared::Nothing);
                };
                // Pins the user's identity key on first sight; a changed key
                // fails with IdentityKeyChanged (after the warning event) and
                // the add waits for the admin to accept it.
                let checked = self.checked_key_packages(user, &response)?;
                let wanted = device.to_vec();
                let Some((key_package, recipient)) = checked
                    .into_iter()
                    .find(|(_, recipient)| recipient.device_id == wanted)
                else {
                    return Ok(Prepared::Nothing);
                };
                let staged = self.stage(CommitPlan {
                    adds: vec![key_package],
                    ..Default::default()
                })?;
                Ok(Prepared::Commit(staged, vec![recipient]))
            }
            Job::AddLinked { device } => {
                if self.group.is_none() || self.store().group()?.is_none() {
                    // No group yet: it stays pending until there is one.
                    return Err(ClientError::NotYet);
                }
                self.usable_group()?;
                let group = self.group.as_ref().ok_or(ClientError::NoGroup)?;
                if leaf_of(group, device).is_some() {
                    self.store().remove_pending_link(device)?;
                    return Ok(Prepared::Nothing);
                }
                let user = self.user();
                let Some(response) = self.fetch(&user, &[*device]).await? else {
                    // Revoked meanwhile.
                    self.store().remove_pending_link(device)?;
                    return Ok(Prepared::Nothing);
                };
                if response.devices.is_empty() {
                    // No KeyPackages yet: the next poll tries again. Keep it
                    // pending by not finishing the link.
                    return Err(ClientError::NotYet);
                }
                let checked = self.checked_key_packages(&user, &response)?;
                let (adds, recipients): (Vec<_>, Vec<_>) = checked.into_iter().unzip();
                if adds.is_empty() {
                    return Ok(Prepared::Nothing);
                }
                let staged = self.stage(CommitPlan {
                    adds,
                    ..Default::default()
                })?;
                Ok(Prepared::Commit(staged, recipients))
            }
            Job::CommitServerRemoves => {
                self.usable_group()?;
                let group = self.group.as_ref().ok_or(ClientError::NoGroup)?;
                if !membership::has_pending_server_removes(group, self.server_index()) {
                    return Ok(Prepared::Nothing);
                }
                let staged = self.stage(CommitPlan::default())?;
                Ok(Prepared::Commit(staged, Vec::new()))
            }
            Job::Rotate => {
                self.usable_group()?;
                let Some(row) = self.store().group()? else {
                    return Ok(Prepared::Nothing);
                };
                if !crate::keys::rotation_due(row.own_leaf_updated_at, self.now()) {
                    return Ok(Prepared::Nothing);
                }
                let staged = self.stage(CommitPlan {
                    force_path: true,
                    ..Default::default()
                })?;
                Ok(Prepared::Commit(staged, Vec::new()))
            }
            Job::AddToDm {
                group_id,
                user,
                device,
            } => {
                let Some(group) = self.dms.get(group_id) else {
                    // Closed, or not joined (yet): nothing to add to.
                    return Ok(Prepared::Nothing);
                };
                let current = leaf_fingerprint(group, device);
                let pending = pending_removals(group);
                if let Some((leaf, _)) = &current
                    && !pending.contains(leaf)
                {
                    tracing::info!("the device is in the DM already; nothing to add");
                    return Ok(Prepared::Nothing);
                }
                let Some(response) = self.fetch_in(user, &[*device], group_id).await? else {
                    return Ok(Prepared::Nothing);
                };
                self.check_pin(user, &response.identity_public_key)?;
                let mut checked = None;
                for entry in &response.devices {
                    let (key_package, cert) = check_fetched(
                        self.store().mls(),
                        &response.identity_public_key,
                        user,
                        entry,
                    )
                    .map_err(|problem| ClientError::Certificate(problem.to_string()))?;
                    if cert.device_id == *device {
                        checked = Some((key_package, device_ref(user, device)));
                    }
                }
                let Some((key_package, recipient)) = checked else {
                    return Ok(Prepared::Nothing);
                };
                // The server's pending Remove of an old leaf goes in by
                // reference (the stage consumes the proposal store).
                let staged = self.stage_in(
                    group_id,
                    CommitPlan {
                        adds: vec![key_package],
                        ..Default::default()
                    },
                )?;
                Ok(Prepared::Commit(staged, vec![recipient]))
            }
            Job::CommitServerRemovesIn { group_id } => {
                let server = self.server_index_in(group_id);
                let Some(group) = self.dms.get(group_id) else {
                    return Ok(Prepared::Nothing);
                };
                if !membership::has_pending_server_removes(group, server) {
                    return Ok(Prepared::Nothing);
                }
                let staged = self.stage_in(group_id, CommitPlan::default())?;
                Ok(Prepared::Commit(staged, Vec::new()))
            }
            Job::RotateIn { group_id } => {
                let Some(dm) = self.store().dm_by_group(group_id)? else {
                    return Ok(Prepared::Nothing);
                };
                if !self.dms.contains_key(group_id)
                    || dm.state.stalled.is_some()
                    || !crate::keys::rotation_due(dm.state.own_leaf_updated_at, self.now())
                {
                    return Ok(Prepared::Nothing);
                }
                let staged = self.stage_in(
                    group_id,
                    CommitPlan {
                        force_path: true,
                        ..Default::default()
                    },
                )?;
                Ok(Prepared::Commit(staged, Vec::new()))
            }
            Job::AddToGroup { group_id, user, .. } => {
                self.require_admin()?;
                let Some(group) = self.dms.get(group_id) else {
                    return Err(ClientError::NoSuchChat);
                };
                let Some(response) = self.fetch_in(user, &[], group_id).await? else {
                    return Err(ClientError::UnknownUser(user.clone()));
                };
                self.check_pin(user, &response.identity_public_key)?;
                let mut checked = Vec::new();
                for entry in &response.devices {
                    let (key_package, cert) = check_fetched(
                        self.store().mls(),
                        &response.identity_public_key,
                        user,
                        entry,
                    )
                    .map_err(|problem| ClientError::Certificate(problem.to_string()))?;
                    if leaf_of(group, &cert.device_id).is_some() {
                        continue;
                    }
                    checked.push((key_package, device_ref(user, &cert.device_id)));
                }
                if checked.is_empty() {
                    return if response.devices.is_empty() {
                        Err(ClientError::NoKeyPackages(user.clone()))
                    } else {
                        Ok(Prepared::Nothing)
                    };
                }
                let (adds, recipients): (Vec<_>, Vec<_>) = checked.into_iter().unzip();
                let staged = self.stage_in(
                    group_id,
                    CommitPlan {
                        adds,
                        ..Default::default()
                    },
                )?;
                Ok(Prepared::Commit(staged, recipients))
            }
            Job::RemoveFromGroup { group_id, user, .. } => {
                self.require_admin()?;
                let Some(group) = self.dms.get(group_id) else {
                    return Ok(Prepared::Nothing);
                };
                let pending = pending_removals(group);
                let removes: Vec<LeafNodeIndex> = leaves_of_user(group, user)
                    .into_iter()
                    .map(|(leaf, _)| leaf)
                    .filter(|leaf| !pending.contains(leaf))
                    .collect();
                if removes.is_empty() {
                    return Ok(Prepared::Nothing);
                }
                let staged = self.stage_in(
                    group_id,
                    CommitPlan {
                        removes,
                        ..Default::default()
                    },
                )?;
                Ok(Prepared::Commit(staged, Vec::new()))
            }
            Job::CreateGroup { .. }
            | Job::CreateNamedGroup { .. }
            | Job::CloseGroup { .. }
            | Job::RemoveClosedDm { .. }
            | Job::UploadKeyPackages { .. }
            | Job::RequestReadd { .. }
            | Job::OpenDm { .. } => Ok(Prepared::Nothing),
        }
    }

    /// The id of an open named group this device is in that `user` has a
    /// device in, if any.
    fn named_group_with(&self, user: &UserId) -> Option<Vec<u8>> {
        self.dms.iter().find_map(|(group_id, group)| {
            (mls_group::is_named(group.extensions()) && !leaves_of_user(group, user).is_empty())
                .then(|| group_id.clone())
        })
    }

    /// multi-group: Named group creation. The group at epoch 0 with this
    /// device (the admin's) as its only member, then `Admin.CreateNamedGroup`;
    /// afterwards the name and the member adds are queued.
    async fn create_named_group(&mut self, job: &Job) -> Result<(), ClientError> {
        let Job::CreateNamedGroup {
            group_id,
            name,
            members,
            ..
        } = job
        else {
            return Ok(());
        };
        self.require_admin()?;
        if !self
            .shared
            .server_knows_named_groups
            .load(std::sync::atomic::Ordering::Relaxed)
        {
            return Err(ClientError::GroupsUnavailable);
        }
        let session = self.session().map_err(ClientError::from)?;
        let admin = self.user().to_string();
        let signer = self.keys.mls_signer();
        let mut group = mls_group::create_named_group(
            self.store().mls(),
            &signer,
            self.credential(),
            openmls::prelude::GroupId::from_slice(group_id),
            self.server.clone(),
            &admin,
        )?;
        let group_info = group
            .export_group_info(self.store().mls().crypto(), &signer, false)
            .map_err(|e| crate::mls::MlsError::new("exporting the GroupInfo", e))?;
        let request = pb::CreateNamedGroupRequest {
            group_id: group.group_id().as_slice().to_vec(),
            group_info: mls_group::wire(&group_info)?,
            ratchet_tree: group
                .export_ratchet_tree()
                .tls_serialize_detached()
                .map_err(|e| crate::mls::MlsError::new("serializing the ratchet tree", e))?,
        };
        let created = session
            .conn()
            .admin()
            .create_named_group(session.request(request)?)
            .await;
        if let Err(status) = created {
            let _ = group.delete(self.store().mls().storage());
            return Err(NetError::from(status).into());
        }
        let now = self.now();
        let chat = self
            .store()
            .set_named_joined(group.group_id().as_slice(), now, 0)?;
        tracing::info!("created a named group");
        self.dms.insert(group.group_id().as_slice().to_vec(), group);
        if let Some(name) = name {
            self.store().set_named_name(group_id, name)?;
            self.queue_named_name(chat, name)?;
        }
        for user in members {
            self.push_job(Job::AddToGroup {
                group_id: group_id.clone(),
                user: user.clone(),
                reply: None,
            });
        }
        self.emit(crate::events::Event::ChatsChanged);
        Ok(())
    }

    /// Queues a `GroupSettings` with `name` in the named group's chat `chat`.
    pub(super) fn queue_named_name(&self, chat: i64, name: &str) -> Result<(), ClientError> {
        let now = self.now();
        let content = crate::messaging::encode_group_settings(Some(name), now)
            .map_err(|_| ClientError::TooLong)?;
        let user = self.user().to_string();
        self.store().insert_outgoing_in(
            chat,
            &crate::store::OutgoingMessage {
                msg_id: super::random_msg_id()?,
                sender_user: &user,
                sender_device: self.account.device_id,
                content: &content,
                created_at: now,
            },
        )?;
        Ok(())
    }

    /// multi-group: Closing a named group. `Admin.CloseGroup`; the
    /// `GroupClosed` notice then marks the chat closed everywhere, this device
    /// included.
    async fn close_named_group(&mut self, group_id: &[u8]) -> Result<(), ClientError> {
        self.require_admin()?;
        let session = self.session().map_err(ClientError::from)?;
        session
            .conn()
            .admin()
            .close_group(session.request(pb::CloseGroupRequest {
                group_id: group_id.to_vec(),
            })?)
            .await
            .map_err(NetError::from)?;
        tracing::info!("closed a named group");
        Ok(())
    }

    /// message-deletion: Remove a closed DM. The server first, so the removal
    /// reaches this user's other devices; only then the local delete.
    async fn remove_closed_dm(&mut self, group_id: &[u8]) -> Result<(), ClientError> {
        let session = self.session().map_err(ClientError::from)?;
        session
            .conn()
            .dms()
            .remove_closed_dm(session.request(pb::RemoveClosedDmRequest {
                group_id: group_id.to_vec(),
            })?)
            .await
            .map_err(NetError::from)?;
        self.apply_removed(group_id)?;
        tracing::info!("removed a closed DM");
        Ok(())
    }

    /// Deletes the closed DM `group_id` locally (idempotent; a chat that is
    /// open, a group or unknown is left alone).
    pub(crate) fn apply_removed(
        &mut self,
        group_id: &[u8],
    ) -> Result<(), crate::store::StoreError> {
        if self.store().remove_closed_dm(group_id)? {
            self.dms.remove(group_id);
            self.emit(crate::events::Event::ChatsChanged);
        }
        Ok(())
    }

    fn server_index_in(&self, group_id: &[u8]) -> Option<u32> {
        self.group_ref(group_id)
            .and_then(|g| mls_group::pi_sender_index(g.extensions(), &self.server))
    }

    /// direct-messages: Opening a DM. The DM at epoch 0 with this device as
    /// its only member, then `Dms.CreateDm`. When the peer opened it first,
    /// the server answers with theirs: this device drops its own and waits for
    /// the Welcome.
    async fn open_dm(&mut self, peer: &UserId) -> Result<(), ClientError> {
        if !self
            .shared
            .server_knows_dms
            .load(std::sync::atomic::Ordering::Relaxed)
        {
            return Err(ClientError::DmsUnavailable);
        }
        if *peer == self.user() {
            return Err(ClientError::UnknownUser(peer.clone()));
        }
        if self.store().open_dm_with(&peer.to_string())?.is_some() {
            return Ok(());
        }
        let session = self.session().map_err(ClientError::from)?;
        let group_id = mls_group::new_group_id()?;
        let own = self.user().to_string();
        let other = peer.to_string();
        let signer = self.keys.mls_signer();
        let mut group = mls_group::create_dm(
            self.store().mls(),
            &signer,
            self.credential(),
            group_id,
            self.server.clone(),
            (&own, &other),
        )?;
        let group_info = group
            .export_group_info(self.store().mls().crypto(), &signer, false)
            .map_err(|e| crate::mls::MlsError::new("exporting the GroupInfo", e))?;
        let request = pb::CreateDmRequest {
            peer: Some(peer.to_pb()),
            group_id: group.group_id().as_slice().to_vec(),
            group_info: mls_group::wire(&group_info)?,
            ratchet_tree: group
                .export_ratchet_tree()
                .tls_serialize_detached()
                .map_err(|e| crate::mls::MlsError::new("serializing the ratchet tree", e))?,
        };
        let answer = session
            .conn()
            .dms()
            .create_dm(session.request(request)?)
            .await;
        let answer = match answer {
            Ok(answer) => answer.into_inner(),
            Err(status) => {
                let _ = group.delete(self.store().mls().storage());
                if status.code() == tonic::Code::NotFound {
                    return Err(ClientError::UnknownUser(peer.clone()));
                }
                return Err(NetError::from(status).into());
            }
        };
        let now = self.now();
        if answer.created {
            self.store()
                .set_dm_joined(group.group_id().as_slice(), &other, now, 0)?;
            tracing::info!("opened a DM");
            self.dms.insert(group.group_id().as_slice().to_vec(), group);
        } else {
            let _ = group.delete(self.store().mls().storage());
            self.store().set_dm_waiting(&answer.group_id, &other, now)?;
            tracing::info!("the peer opened the DM first; waiting for its Welcome");
        }
        self.emit(crate::events::Event::ChatsChanged);
        Ok(())
    }

    fn server_index(&self) -> Option<u32> {
        self.group
            .as_ref()
            .and_then(|g| mls_group::pi_sender_index(g.extensions(), &self.server))
    }

    fn require_admin(&self) -> Result<(), ClientError> {
        if self.account.is_admin {
            Ok(())
        } else {
            Err(ClientError::NotAdmin)
        }
    }

    fn credential(&self) -> openmls::prelude::CredentialWithKey {
        credential_with_key(
            &pb::SignedDeviceCertificate {
                certificate: self.account.certificate.clone(),
                signature: self.account.certificate_signature.clone(),
            },
            &self.keys.mls_public(),
        )
    }

    /// group-membership: Group creation. The group at epoch 0 with this
    /// device as its only member, then `Admin.CreateGroup` with its public
    /// state.
    async fn create_group(&mut self) -> Result<(), ClientError> {
        self.require_admin()?;
        if self.store().group()?.is_some() {
            return Err(ClientError::AlreadyInGroup);
        }
        let session = self.session().map_err(ClientError::from)?;
        let group_id = mls_group::new_group_id()?;
        let admin = self.user().to_string();
        let signer = self.keys.mls_signer();
        let mut group = mls_group::create(
            self.store().mls(),
            &signer,
            self.credential(),
            group_id,
            self.server.clone(),
            &admin,
        )?;
        let group_info = group
            .export_group_info(self.store().mls().crypto(), &signer, false)
            .map_err(|e| crate::mls::MlsError::new("exporting the GroupInfo", e))?;
        let request = pb::CreateGroupRequest {
            group_id: group.group_id().as_slice().to_vec(),
            group_info: mls_group::wire(&group_info)?,
            ratchet_tree: group
                .export_ratchet_tree()
                .tls_serialize_detached()
                .map_err(|e| crate::mls::MlsError::new("serializing the ratchet tree", e))?,
        };
        let created = session
            .conn()
            .admin()
            .create_group(session.request(request)?)
            .await;
        if let Err(status) = created {
            let _ = group.delete(self.store().mls().storage());
            // The server already holds a lobby (e.g. a linked admin device that
            // has not synced yet raced an auto-create): report it as
            // AlreadyInGroup so the caller can treat it as benign and wait to be
            // synced in, not as a network error (app-ui: The lobby appears
            // without a manual create step).
            if status.code() == tonic::Code::AlreadyExists {
                return Err(ClientError::AlreadyInGroup);
            }
            return Err(NetError::from(status).into());
        }
        self.store()
            .set_group(group.group_id().as_slice(), self.now(), 0)?;
        tracing::info!("created the group");
        self.group = Some(group);
        self.emit(crate::events::Event::MembersChanged {
            added: Vec::new(),
            removed: Vec::new(),
            by: Some(self.user()),
            automatic: Vec::new(),
        });
        Ok(())
    }

    /// key-directory: KeyPackage upload; Replenishment signal; Last-resort
    /// KeyPackage refresh.
    async fn upload_key_packages(&mut self, remaining: Option<u32>) -> Result<(), ClientError> {
        let session = self.session().map_err(ClientError::from)?;
        let count = remaining.map_or(0, crate::keys::top_up);
        let credential = self.credential();
        let signer = self.keys.mls_signer();
        let store = std::sync::Arc::clone(&self.shared.store);
        let upload = store.transaction(|| -> Result<_, EngineError> {
            Ok(crate::keys::new_upload(
                store.mls(),
                &signer,
                &credential,
                count,
            )?)
        })?;
        session
            .conn()
            .keys()
            .upload_key_packages(session.request(pb::UploadKeyPackagesRequest {
                key_packages: upload.one_time,
                last_resort_key_package: upload.last_resort,
            })?)
            .await
            .map_err(NetError::from)?;
        let old = store.add_last_resort(&upload.last_resort_ref, self.now())?;
        for hash_ref in old {
            if let Err(error) = crate::keys::forget(store.mls(), &hash_ref) {
                tracing::warn!(%error, "cannot delete an old last-resort KeyPackage");
            }
        }
        // direct-messages: Old apps never join a DM. These list 0xF0A2.
        store.set_flag(super::DM_KEY_PACKAGES_FLAG, self.now())?;
        tracing::info!(one_time = count, "uploaded KeyPackages");
        Ok(())
    }

    /// group-membership: Re-add after an unusable Welcome. Asks the server with
    /// fresh KeyPackages, which replace the device's stored ones only if the
    /// Server answers `ASKED` (welcome-recovery design D2, D3).
    async fn request_readd(&mut self, refused_seq: u64, reason: &str) -> Result<(), ClientError> {
        if refused_seq == 0 && self.store().group()?.is_some() {
            // Joined since the ask was queued.
            return Ok(());
        }
        let session = self.session().map_err(ClientError::from)?;
        let credential = self.credential();
        let signer = self.keys.mls_signer();
        let store = std::sync::Arc::clone(&self.shared.store);
        let upload = store.transaction(|| -> Result<_, EngineError> {
            Ok(crate::keys::new_upload(
                store.mls(),
                &signer,
                &credential,
                protocol::key_directory::ONE_TIME_KEY_PACKAGES_TARGET,
            )?)
        })?;
        let response = session
            .conn()
            .keys()
            .request_readd(session.request(pb::RequestReaddRequest {
                refused_welcome_seq: refused_seq,
                reason: protocol::group_membership::readd_reason(reason).to_owned(),
                key_packages: upload.one_time.clone(),
                last_resort_key_package: upload.last_resort.clone(),
            })?)
            .await;
        let response = match response {
            Ok(response) => response.into_inner(),
            Err(status) => {
                forget_unused(&store, &upload);
                if status.code() == tonic::Code::Unimplemented {
                    // A server from before welcome-recovery: wait as before.
                    tracing::warn!("the server cannot re-add this device on request");
                    return Ok(());
                }
                return Err(NetError::from(status).into());
            }
        };
        let outcome = match pb::ReaddOutcome::try_from(response.outcome) {
            Ok(pb::ReaddOutcome::Asked) => ReaddOutcome::Asked,
            Ok(pb::ReaddOutcome::NotAMember) => ReaddOutcome::NotAMember,
            Ok(pb::ReaddOutcome::WelcomeQueued) => ReaddOutcome::WelcomeQueued,
            Ok(pb::ReaddOutcome::AlreadyWaiting) => ReaddOutcome::AlreadyWaiting,
            Ok(pb::ReaddOutcome::LimitReached) => ReaddOutcome::LimitReached,
            Ok(pb::ReaddOutcome::Unspecified) | Err(_) => {
                forget_unused(&store, &upload);
                return Err(NetError::Protocol("RequestReadd without an outcome".into()).into());
            }
        };
        if outcome == ReaddOutcome::Asked {
            let old = store.add_last_resort(&upload.last_resort_ref, self.now())?;
            for hash_ref in old {
                if let Err(error) = crate::keys::forget(store.mls(), &hash_ref) {
                    tracing::warn!(%error, "cannot delete an old last-resort KeyPackage");
                }
            }
        } else {
            forget_unused(&store, &upload);
        }
        tracing::info!(refused_seq, ?outcome, "asked to be added again");
        self.emit(crate::events::Event::ReaddRequested { outcome });
        Ok(())
    }
}

/// Deletes the private keys of KeyPackages the server did not take.
fn forget_unused(store: &crate::store::Store, upload: &crate::keys::Upload) {
    let forgot = store.transaction(|| -> Result<(), EngineError> {
        for hash_ref in upload
            .one_time_refs
            .iter()
            .chain(std::iter::once(&upload.last_resort_ref))
        {
            crate::keys::forget(store.mls(), hash_ref)?;
        }
        Ok(())
    });
    if let Err(error) = forgot {
        tracing::warn!(%error, "cannot delete unused KeyPackages");
    }
}
