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
use crate::events::{DeviceId, UserId};
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
    /// device-identity: Device linking. Add a device this device linked,
    /// once it has uploaded KeyPackages.
    AddLinked { device: DeviceId },
    /// Admin: commit the Pi's pending inactivity Removes.
    CommitPiRemoves,
    /// group-membership: Own leaf rotation.
    Rotate,
    /// key-directory: upload KeyPackages. `Some(n)`: top up one-time
    /// KeyPackages from `n` (signup, linking, a low notice); `None`: only
    /// replace the last-resort one (weekly refresh). Every upload carries a
    /// new last-resort KeyPackage.
    UploadKeyPackages { remaining: Option<u32> },
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
                | Job::AddLinked { .. }
                | Job::CommitPiRemoves
                | Job::Rotate
                | Job::UploadKeyPackages { .. }
        )
    }

    pub(crate) fn same_as(&self, other: &Job) -> bool {
        match (self, other) {
            (Job::Readd { device: a, .. }, Job::Readd { device: b, .. }) => a == b,
            (Job::AddLinked { device: a }, Job::AddLinked { device: b }) => a == b,
            (Job::CommitPiRemoves, Job::CommitPiRemoves)
            | (Job::Rotate, Job::Rotate)
            | (Job::UploadKeyPackages { .. }, Job::UploadKeyPackages { .. }) => true,
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
            Job::AddLinked { device } => format!("add_linked|{}", hex(device)),
            Job::CommitPiRemoves => "pi_removes".into(),
            Job::Rotate => "rotate".into(),
            Job::UploadKeyPackages { .. } => "upload_key_packages".into(),
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
            ["add_linked", device] => parse_device(device).map(|device| Job::AddLinked { device }),
            ["pi_removes"] => Some(Job::CommitPiRemoves),
            _ => None,
        };
        job.unwrap_or(Job::Rotate)
    }

    fn take_reply(&mut self) -> Option<Reply> {
        match self {
            Job::CreateGroup { reply }
            | Job::AddUser { reply, .. }
            | Job::RemoveUser { reply, .. }
            | Job::RemoveDevice { reply, .. } => reply.take(),
            _ => None,
        }
    }

    /// Ends the job with `error`.
    pub(crate) fn fail(mut self, error: ClientError) {
        match self.take_reply() {
            Some(reply) => {
                let _ = reply.send(Err(error));
            }
            None => tracing::warn!(purpose = %self.purpose(), %error, "job failed"),
        }
    }

    fn succeed(mut self) {
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

    /// What to do after the Pi accepted a job's commit (or the job needed
    /// none).
    pub(super) async fn finish(&mut self, job: Job) {
        match job {
            Job::RemoveUser { ref user, .. } => {
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

    /// Stages a commit in a store transaction; reloads the group if it fails.
    fn stage(&mut self, plan: CommitPlan) -> Result<StagedOwnCommit, ClientError> {
        let store = std::sync::Arc::clone(&self.shared.store);
        let signer = self.keys.mls_signer();
        let Some(group) = self.group.as_mut() else {
            return Err(ClientError::NoGroup);
        };
        let staged = store.transaction(|| -> Result<StagedOwnCommit, EngineError> {
            Ok(membership::stage(group, store.mls(), &signer, plan)?)
        });
        match staged {
            Ok(staged) => Ok(staged),
            Err(error) => {
                self.reload_group().map_err(ClientError::from)?;
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
        let session = self.session().map_err(ClientError::from)?;
        let response = session
            .conn()
            .keys()
            .fetch_key_packages(session.request(pb::FetchKeyPackagesRequest {
                user_id: Some(user.to_pb()),
                device_ids: devices.iter().map(|d| d.to_vec()).collect(),
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
                if removes.is_empty() && !membership::has_pending_pi_removes(group, self.pi_index())
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
            Job::CommitPiRemoves => {
                self.usable_group()?;
                let group = self.group.as_ref().ok_or(ClientError::NoGroup)?;
                if !membership::has_pending_pi_removes(group, self.pi_index()) {
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
            Job::CreateGroup { .. } | Job::UploadKeyPackages { .. } => Ok(Prepared::Nothing),
        }
    }

    fn pi_index(&self) -> Option<u32> {
        self.group
            .as_ref()
            .and_then(|g| mls_group::pi_sender_index(g.extensions(), &self.pi))
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
            self.pi.clone(),
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
            return Err(NetError::from(status).into());
        }
        self.store()
            .set_group(group.group_id().as_slice(), self.now(), 0)?;
        tracing::info!("created the group");
        self.group = Some(group);
        self.emit(crate::events::Event::MembersChanged);
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
        tracing::info!(one_time = count, "uploaded KeyPackages");
        Ok(())
    }
}
