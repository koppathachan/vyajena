//! Admin service: invites, the group and user removal (pi-server-core tasks
//! 6.1, 6.2). Every RPC is for admin devices only; any other device gets
//! `PERMISSION_DENIED` (device-identity: Invites). The bootstrap invite
//! (device-identity: Bootstrap admin) uses [`invites`] at first start.

pub mod invites;

use tonic::{Request, Response, Status};

use crate::auth::{self, Caller, caller};
use crate::membership::{self, GroupState};
use crate::pb;
use crate::pb::admin_server::{Admin, AdminServer};
use crate::runtime::State;
use crate::runtime::clock::{to_timestamp, unix_millis};
use crate::runtime::config::user_id_part_problem;
use crate::store::records;

/// The gRPC service.
pub fn service(state: &State) -> AdminServer<AdminService> {
    AdminServer::new(AdminService {
        state: state.clone(),
    })
}

/// Implements `vyajena.v1.Admin`.
pub struct AdminService {
    state: State,
}

/// The authenticated caller, if it is an admin device.
fn admin<T>(request: &Request<T>) -> Result<Caller, Status> {
    let caller = caller(request)?;
    if !caller.is_admin {
        return Err(Status::permission_denied(
            "only the admin's devices may do this",
        ));
    }
    Ok(caller.clone())
}

impl AdminService {
    fn to_pb(&self, invite: &invites::Invite) -> pb::Invite {
        pb::Invite {
            code: invite.code.clone(),
            user_id: Some(pb::UserId {
                username: invite.username.clone(),
                server_id: self.state.config.server_id.clone(),
            }),
            created_at: Some(to_timestamp(invite.created_at)),
            expires_at: Some(to_timestamp(invite.expires_at)),
        }
    }
}

#[tonic::async_trait]
impl Admin for AdminService {
    async fn create_invite(
        &self,
        request: Request<pb::CreateInviteRequest>,
    ) -> Result<Response<pb::CreateInviteResponse>, Status> {
        admin(&request)?;
        let username = request.into_inner().username;
        if let Some(problem) = user_id_part_problem(&username) {
            return Err(Status::invalid_argument(format!("username {problem}")));
        }
        let now = self.state.clock.now();
        let invite = self
            .state
            .store
            .call(move |conn| {
                let tx = conn.transaction()?;
                // A removed user's username is free again (group-membership:
                // Removing a user).
                if records::user(&tx, &username)?.is_some_and(|user| !user.removed) {
                    return Ok(Err(Status::already_exists(
                        "a user with this username exists",
                    )));
                }
                let invite = invites::create(&tx, &username, now, false)?;
                tx.commit()?;
                Ok(Ok(invite))
            })
            .await??;
        tracing::info!(username = %invite.username, "created an invite");
        let invite = self.to_pb(&invite);
        let payload = pb::InvitePayload {
            relay_address: self.state.config.public_address.clone(),
            tls_spki_sha256: self.state.keys.tls_spki_sha256().to_vec(),
            invite_code: invite.code.clone(),
            user_id: invite.user_id.clone(),
        };
        Ok(Response::new(pb::CreateInviteResponse {
            invite: Some(invite),
            payload: Some(payload),
        }))
    }

    async fn list_invites(
        &self,
        request: Request<pb::ListInvitesRequest>,
    ) -> Result<Response<pb::ListInvitesResponse>, Status> {
        admin(&request)?;
        let now = self.state.clock.now();
        let list = self
            .state
            .store
            .call(move |conn| invites::list_unused(&conn.transaction()?, now))
            .await?;
        Ok(Response::new(pb::ListInvitesResponse {
            invites: list.iter().map(|invite| self.to_pb(invite)).collect(),
        }))
    }

    async fn revoke_invite(
        &self,
        request: Request<pb::RevokeInviteRequest>,
    ) -> Result<Response<pb::RevokeInviteResponse>, Status> {
        admin(&request)?;
        let code = request.into_inner().code;
        let now = self.state.clock.now();
        let outcome = self
            .state
            .store
            .call(move |conn| {
                let tx = conn.transaction()?;
                let outcome = invites::revoke(&tx, &code, now)?;
                tx.commit()?;
                Ok(outcome)
            })
            .await?;
        match outcome {
            invites::RevokeOutcome::Revoked(username) => {
                tracing::info!(username = %username, "revoked an invite");
                Ok(Response::new(pb::RevokeInviteResponse {}))
            }
            invites::RevokeOutcome::NotFound => Err(Status::not_found("no invite with this code")),
            invites::RevokeOutcome::AlreadyUsed => {
                Err(Status::failed_precondition("the invite was already used"))
            }
        }
    }

    async fn create_group(
        &self,
        request: Request<pb::CreateGroupRequest>,
    ) -> Result<Response<pb::CreateGroupResponse>, Status> {
        let caller = admin(&request)?;
        let request = request.into_inner();
        let admin_user_id = self.state.config.admin_user_id();
        let pi = membership::external_sender(&self.state.config.server_id, &self.state.keys);
        let now = self.state.clock.now();
        self.state
            .store
            .call(move |conn| {
                let tx = conn.transaction()?;
                match GroupState::create(&tx, &caller, &admin_user_id, &pi, &request, now) {
                    Ok(_) => {
                        tx.commit()?;
                        Ok(Ok(()))
                    }
                    Err(membership::CreateGroupError::Store(error)) => Err(error),
                    Err(error) => Ok(Err(Status::from(error))),
                }
            })
            .await??;
        tracing::info!("created the group at epoch 0");
        Ok(Response::new(pb::CreateGroupResponse {}))
    }

    async fn remove_user(
        &self,
        request: Request<pb::RemoveUserRequest>,
    ) -> Result<Response<pb::RemoveUserResponse>, Status> {
        admin(&request)?;
        let user_id = request
            .into_inner()
            .user_id
            .ok_or_else(|| Status::invalid_argument("user_id is required"))?;
        if user_id.server_id != self.state.config.server_id {
            return Err(Status::not_found("unknown user"));
        }
        if user_id.username == self.state.config.admin_username {
            return Err(Status::failed_precondition("the admin cannot be removed"));
        }
        let now = self.state.clock.now();
        let (revoked, invites) = self
            .state
            .store
            .call(move |conn| {
                let tx = conn.transaction()?;
                if records::user(&tx, &user_id.username)?.is_none() {
                    return Ok(Err(Status::not_found("unknown user")));
                }
                let devices = records::active_devices(&tx, &user_id.username)?;
                for device in &devices {
                    auth::revoke_device(&tx, &device.device_id, now)?;
                }
                tx.execute(
                    "UPDATE users SET removed_at = coalesce(removed_at, ?2) WHERE username = ?1",
                    rusqlite::params![user_id.username, unix_millis(now)],
                )?;
                let invites = invites::revoke_unused_for(&tx, &user_id.username, now)?;
                tx.commit()?;
                Ok(Ok((
                    devices.into_iter().map(|d| d.device_id).collect::<Vec<_>>(),
                    invites,
                )))
            })
            .await??;
        for device_id in &revoked {
            let _ = self.state.revocations.send(*device_id);
        }
        tracing::info!(devices = revoked.len(), invites, "removed a user");
        Ok(Response::new(pb::RemoveUserResponse {}))
    }
}
