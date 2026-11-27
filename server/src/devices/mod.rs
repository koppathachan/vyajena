//! Devices service: Server info, signup, challenge and authentication,
//! linking, revocation and push tokens (server-core tasks 4.1-4.3).

use std::time::SystemTime;

use protocol::device_identity::DeviceId;
use protocol::device_identity::{PushTokenKind, push_token_problem};
use protocol::wire_transport::{
    CHALLENGE_NONCE_LEN, MAX_APPLICATION_CIPHERTEXT_BYTES, MAX_HANDSHAKE_ENVELOPE_BYTES,
    SUPPORTED_PROTOCOL_VERSIONS, verify_auth_signature,
};
use rusqlite::{OptionalExtension, Transaction, params};
use tonic::{Request, Response, Status};

use crate::auth::{self, caller};
use crate::membership::{self, certificate};
use crate::pb;
use crate::pb::devices_server::{Devices, DevicesServer};
use crate::runtime::clock::{from_unix_millis, to_timestamp, unix_millis};
use crate::runtime::{State, TlsConnectInfo};
use crate::store::StoreError;
use crate::store::records::{self, DeviceRow};

/// The gRPC service.
pub fn service(state: &State) -> DevicesServer<DevicesService> {
    DevicesServer::new(DevicesService {
        state: state.clone(),
    })
}

/// Implements `vyajena.v1.Devices`.
pub struct DevicesService {
    state: State,
}

/// What `Authenticate` found inside its store call.
enum Authenticated {
    /// A session token and its expiry.
    Issued((String, SystemTime)),
    /// A valid signature from a revoked device (`PERMISSION_DENIED`).
    Revoked,
    /// Unknown, used or expired nonce, unknown device or bad signature.
    Rejected,
}

/// Stores a new device from a verified certificate.
fn insert_device(
    tx: &Transaction<'_>,
    cert: &certificate::VerifiedCertificate,
    now: SystemTime,
) -> Result<(), Status> {
    if records::device(tx, &cert.device_id)?.is_some() {
        return Err(Status::already_exists("this device is already registered"));
    }
    tx.execute(
        "INSERT INTO devices (device_id, username, auth_public_key, mls_signature_public_key,
                              certificate, certificate_signature, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            cert.device_id.as_slice(),
            cert.username,
            cert.auth_public_key,
            cert.mls_signature_public_key,
            cert.certificate,
            cert.signature,
            unix_millis(now),
        ],
    )
    .map_err(StoreError::from)?;
    Ok(())
}

fn signed_certificate(device: &DeviceRow) -> pb::SignedDeviceCertificate {
    pb::SignedDeviceCertificate {
        certificate: device.certificate.clone(),
        signature: device.certificate_signature.clone(),
    }
}

#[tonic::async_trait]
impl Devices for DevicesService {
    async fn get_server_info(
        &self,
        _request: Request<pb::GetServerInfoRequest>,
    ) -> Result<Response<pb::GetServerInfoResponse>, Status> {
        let server_id = &self.state.config.server_id;
        let as_u32 = |n: usize| u32::try_from(n).unwrap_or(u32::MAX);
        Ok(Response::new(pb::GetServerInfoResponse {
            server_info: Some(pb::ServerInfo {
                server_id: server_id.clone(),
                supported_protocol_versions: SUPPORTED_PROTOCOL_VERSIONS.to_vec(),
                limits: Some(pb::Limits {
                    max_application_ciphertext_bytes: as_u32(MAX_APPLICATION_CIPHERTEXT_BYTES),
                    max_handshake_envelope_bytes: as_u32(MAX_HANDSHAKE_ENVELOPE_BYTES),
                    // video-messages: Config-driven limit. The one configured blob
                    // cap, so clients refuse a video whose encrypted blob exceeds it.
                    max_video_blob_bytes: self.state.config.limits.max_video_blob_bytes,
                }),
                external_sender: Some(pb::ExternalSender {
                    signature_public_key: self.state.keys.external_sender_public_key().to_vec(),
                    credential: membership::external_sender_credential(server_id)?,
                }),
            }),
        }))
    }

    async fn register(
        &self,
        request: Request<pb::RegisterRequest>,
    ) -> Result<Response<pb::RegisterResponse>, Status> {
        let request = request.into_inner();
        let signed = request
            .certificate
            .ok_or_else(|| Status::invalid_argument("certificate is required"))?;
        let server_id = self.state.config.server_id.clone();
        let reusable_on = self.state.config.review.reusable_admin_invite;
        let now = self.state.clock.now();
        let (username, revoked) = self
            .state
            .store
            .call(move |conn| {
                let tx = conn.transaction()?;
                let result = register(
                    &tx,
                    &request.invite_code,
                    &request.identity_public_key,
                    &signed,
                    &server_id,
                    reusable_on,
                    now,
                );
                if result.is_ok() {
                    tx.commit()?;
                }
                Ok(result)
            })
            .await??;
        // device-identity: Re-inviting an existing user. A re-key revokes the
        // user's prior devices; tear down any live Connect stream now so a
        // surviving device stops at once instead of only on its next
        // Authenticate.
        for device_id in &revoked {
            let _ = self.state.revocations.send(*device_id);
        }
        if reusable_on && username == self.state.config.admin_username {
            tracing::info!(
                %username,
                revoked = revoked.len(),
                "the reusable review admin invite took over the admin seat"
            );
        } else if revoked.is_empty() {
            tracing::info!(%username, "registered a user and their first device");
        } else {
            tracing::info!(%username, revoked = revoked.len(), "re-keyed a user on re-invite");
        }
        Ok(Response::new(pb::RegisterResponse {
            user_id: Some(pb::UserId {
                username,
                server_id: self.state.config.server_id.clone(),
            }),
        }))
    }

    async fn get_challenge(
        &self,
        _request: Request<pb::GetChallengeRequest>,
    ) -> Result<Response<pb::GetChallengeResponse>, Status> {
        let mut nonce = [0u8; CHALLENGE_NONCE_LEN];
        getrandom::fill(&mut nonce).map_err(|_| Status::internal("no randomness"))?;
        let now = self.state.clock.now();
        let expires_at = now + self.state.config.limits.challenge_ttl;
        self.state
            .store
            .call(move |conn| {
                let tx = conn.transaction()?;
                tx.execute(
                    "DELETE FROM challenges WHERE expires_at < ?1",
                    params![unix_millis(now)],
                )?;
                tx.execute(
                    "INSERT INTO challenges (nonce, issued_at, expires_at) VALUES (?1, ?2, ?3)",
                    params![nonce.as_slice(), unix_millis(now), unix_millis(expires_at)],
                )?;
                tx.commit()?;
                Ok(())
            })
            .await?;
        Ok(Response::new(pb::GetChallengeResponse {
            nonce: nonce.to_vec(),
            expires_at: Some(to_timestamp(expires_at)),
        }))
    }

    async fn authenticate(
        &self,
        request: Request<pb::AuthenticateRequest>,
    ) -> Result<Response<pb::AuthenticateResponse>, Status> {
        let tls_exporter = request
            .extensions()
            .get::<TlsConnectInfo>()
            .map(|info| info.tls_exporter)
            .ok_or_else(|| Status::unauthenticated("no TLS exporter for this connection"))?;
        let request = request.into_inner();
        let device = request
            .device
            .ok_or_else(|| Status::unauthenticated("device is required"))?;
        let user = device
            .user_id
            .ok_or_else(|| Status::unauthenticated("device.user_id is required"))?;
        let server_id = self.state.config.server_id.clone();
        let now = self.state.clock.now();
        let issued = self
            .state
            .store
            .call(move |conn| {
                let tx = conn.transaction()?;
                // Single use: the nonce is gone whatever happens next.
                let expires_at: Option<i64> = tx
                    .query_row(
                        "DELETE FROM challenges WHERE nonce = ?1 RETURNING expires_at",
                        params![request.nonce],
                        |row| row.get(0),
                    )
                    .optional()?;
                tx.commit()?;
                let Some(expires_at) = expires_at else {
                    return Ok(Authenticated::Rejected);
                };
                if now > from_unix_millis(expires_at) {
                    return Ok(Authenticated::Rejected);
                }
                let Some(row) = records::device(conn, &device.device_id)? else {
                    return Ok(Authenticated::Rejected);
                };
                if row.username != user.username || user.server_id != server_id {
                    return Ok(Authenticated::Rejected);
                }
                if verify_auth_signature(
                    &row.auth_public_key,
                    &request.nonce,
                    &tls_exporter,
                    &request.signature,
                )
                .is_err()
                {
                    return Ok(Authenticated::Rejected);
                }
                // Only after the signature checks out, so no one without the
                // device's auth key learns whether it is revoked.
                if row.revoked {
                    return Ok(Authenticated::Revoked);
                }
                let tx = conn.transaction()?;
                let issued = auth::issue_session(&tx, &row.device_id, now)?;
                tx.commit()?;
                Ok(Authenticated::Issued(issued))
            })
            .await?;
        let (session_token, expires_at) = match issued {
            Authenticated::Issued(issued) => issued,
            Authenticated::Revoked => {
                return Err(Status::permission_denied("this device was revoked"));
            }
            Authenticated::Rejected => {
                return Err(Status::unauthenticated(
                    "the challenge is unknown, used or expired, or the signature is wrong",
                ));
            }
        };
        Ok(Response::new(pb::AuthenticateResponse {
            session_token,
            expires_at: Some(to_timestamp(expires_at)),
        }))
    }

    async fn link_device(
        &self,
        request: Request<pb::LinkDeviceRequest>,
    ) -> Result<Response<pb::LinkDeviceResponse>, Status> {
        let caller = caller(&request)?.clone();
        let signed = request
            .into_inner()
            .certificate
            .ok_or_else(|| Status::invalid_argument("certificate is required"))?;
        let server_id = self.state.config.server_id.clone();
        let now = self.state.clock.now();
        let device_id = self
            .state
            .store
            .call(move |conn| {
                let tx = conn.transaction()?;
                let result = link(&tx, &caller.username, &server_id, &signed, now);
                if result.is_ok() {
                    tx.commit()?;
                }
                Ok(result)
            })
            .await??;
        tracing::info!(device = %crate::runtime::redact(&device_id), "linked a device");
        Ok(Response::new(pb::LinkDeviceResponse {}))
    }

    async fn get_self(
        &self,
        request: Request<pb::GetSelfRequest>,
    ) -> Result<Response<pb::GetSelfResponse>, Status> {
        let caller = caller(&request)?.clone();
        let device_id = caller.device_id;
        let device = self
            .state
            .store
            .call(move |conn| records::device(conn, &device_id))
            .await?
            .ok_or_else(|| Status::unauthenticated("device not found"))?;
        Ok(Response::new(pb::GetSelfResponse {
            user_id: Some(pb::UserId {
                username: caller.username,
                server_id: self.state.config.server_id.clone(),
            }),
            certificate: Some(signed_certificate(&device)),
            is_admin: caller.is_admin,
        }))
    }

    async fn revoke_device(
        &self,
        request: Request<pb::RevokeDeviceRequest>,
    ) -> Result<Response<pb::RevokeDeviceResponse>, Status> {
        let caller = caller(&request)?.clone();
        let target = records::device_id_from(&request.get_ref().device_id)
            .ok_or_else(|| Status::invalid_argument("device_id must be 16 bytes"))?;
        if target == caller.device_id {
            return Err(Status::invalid_argument(
                "a device revokes another device of its user, not itself",
            ));
        }
        let now = self.state.clock.now();
        self.state
            .store
            .call(move |conn| {
                let tx = conn.transaction()?;
                match records::device(&tx, &target)? {
                    Some(device) if device.username == caller.username => {}
                    _ => return Ok(Err(Status::not_found("no such device of your user"))),
                }
                auth::revoke_device(&tx, &target, now)?;
                tx.commit()?;
                Ok(Ok(()))
            })
            .await??;
        // Its open Connect stream closes (device-identity: Device revocation).
        let _ = self.state.revocations.send(target);
        tracing::info!(device = %crate::runtime::redact(&target), "revoked a device");
        Ok(Response::new(pb::RevokeDeviceResponse {}))
    }

    async fn update_push_token(
        &self,
        request: Request<pb::UpdatePushTokenRequest>,
    ) -> Result<Response<pb::UpdatePushTokenResponse>, Status> {
        let caller = caller(&request)?.clone();
        let token = match request.into_inner().push_token {
            None => None,
            Some(token) => {
                let platform = pb::PushPlatform::try_from(token.platform)
                    .ok()
                    .filter(|p| *p != pb::PushPlatform::Unspecified)
                    .ok_or_else(|| {
                        Status::invalid_argument("push_token.platform must be FCM or APNS")
                    })?;
                let kind = match platform {
                    pb::PushPlatform::Apns => PushTokenKind::Apns,
                    _ => PushTokenKind::Fcm,
                };
                if let Some(problem) = push_token_problem(kind, &token.token) {
                    return Err(Status::invalid_argument(format!(
                        "push_token.token {problem}"
                    )));
                }
                Some((platform as i32, token.token))
            }
        };
        let device_id = caller.device_id;
        self.state
            .store
            .call(move |conn| {
                let (platform, token) = token.unzip();
                conn.execute(
                    "UPDATE devices SET push_platform = ?2, push_token = ?3 WHERE device_id = ?1",
                    params![device_id.as_slice(), platform, token],
                )?;
                Ok(())
            })
            .await?;
        Ok(Response::new(pb::UpdatePushTokenResponse {}))
    }

    // account-deletion: User deletes their own account. The request carries no
    // user id; the server deletes the authenticated caller's own user.
    async fn delete_account(
        &self,
        request: Request<pb::DeleteAccountRequest>,
    ) -> Result<Response<pb::DeleteAccountResponse>, Status> {
        let caller = caller(&request)?.clone();
        // account-deletion: The admin cannot self-delete through the RPC. The
        // admin's only reset path is the operator-only `server admin-reset`.
        if !protocol::account_deletion::may_delete_account(caller.is_admin) {
            return Err(Status::failed_precondition(
                "the admin account cannot be deleted from the app; \
                 reset it with `server admin-reset` on the server host",
            ));
        }
        let now = self.state.clock.now();
        let username = caller.username.clone();
        let server_id = self.state.config.server_id.clone();
        let keys = self.state.keys.clone();
        let limits = self.state.config.limits;
        let (revoked, notify, invites, closed) = self
            .state
            .store
            .call(move |conn| {
                let tx = conn.transaction()?;
                if records::user(&tx, &username)?.is_none() {
                    return Ok(Err(Status::not_found("unknown user")));
                }
                let devices = records::active_devices(&tx, &username)?;
                // Revoke first, so the external-Remove proposals below go only to
                // the group's remaining members (active_members skips revoked and
                // inactive devices), never to the departing user's own devices.
                for device in &devices {
                    auth::revoke_device(&tx, &device.device_id, now)?;
                }
                // Keep only a minimal removed marker (users.removed_at + the
                // username), which frees the username for a fresh invite and
                // keeps the revoked devices revoked.
                tx.execute(
                    "UPDATE users SET removed_at = coalesce(removed_at, ?2) WHERE username = ?1",
                    params![username, unix_millis(now)],
                )?;
                let invites = crate::admin::invites::revoke_unused_for(&tx, &username, now)?;
                // account-deletion: DM peers are told (reuse close_dms_of). Close
                // the DMs first, so the eviction below only runs for the lobby and
                // named groups the user is still in.
                let owed = crate::dms::close_dms_of(&tx, &username, now)?;
                let closed = crate::dms::open_notices(&tx, &owed)?;
                // account-deletion: Deletion evicts the user from every group.
                // Reuse the inactivity external-Remove path for each device: it
                // deletes the device's queued entries and issues the server's
                // external Remove for its leaf in every group, owed to the
                // remaining members who commit it on next connect.
                let ctx = crate::messaging::Context {
                    server_id: &server_id,
                    keys: &keys,
                    now,
                    limits,
                };
                let mut notify = Vec::new();
                for device in &devices {
                    notify.extend(crate::messaging::mark_inactive(
                        &tx,
                        &ctx,
                        &device.device_id,
                    )?);
                    // account-deletion: What the server deletes and what it
                    // retains. The user's own key packages and push token go.
                    tx.execute(
                        "DELETE FROM key_packages WHERE device_id = ?1",
                        params![device.device_id.as_slice()],
                    )?;
                    tx.execute(
                        "UPDATE devices SET push_platform = NULL, push_token = NULL \
                         WHERE device_id = ?1",
                        params![device.device_id.as_slice()],
                    )?;
                }
                tx.commit()?;
                let revoked: Vec<DeviceId> = devices.into_iter().map(|d| d.device_id).collect();
                Ok(Ok((revoked, notify, invites, closed)))
            })
            .await??;
        // Tear down the user's live streams; deliver the external-Remove
        // proposals to the remaining members and the DM-closed notices to peers.
        for device_id in &revoked {
            let _ = self.state.revocations.send(*device_id);
        }
        crate::messaging::notify(&self.state, notify);
        for (device_id, notice) in closed {
            self.state.hub.send_frame(&device_id, notice);
        }
        // account-deletion: purge the user's blobs (reuse Blobs.DeleteBlob
        // semantics; a blob already gone is a safe no-op).
        let uploader_ids = revoked.iter().map(|device_id| device_id.to_vec()).collect();
        crate::blobs::purge_device_blobs(&self.state, uploader_ids).await;
        tracing::info!(
            devices = revoked.len(),
            invites,
            "a user deleted their own account"
        );
        Ok(Response::new(pb::DeleteAccountResponse {}))
    }
}

/// device-identity: Signup with an invite; Admin-owned usernames; Re-inviting
/// an existing user. Returns the user's username and the ids of any prior
/// devices this registration revoked (non-empty only on the re-invite re-key
/// path), so the caller can tear down their live streams.
fn register(
    tx: &Transaction<'_>,
    invite_code: &str,
    identity_public_key: &[u8],
    signed: &pb::SignedDeviceCertificate,
    server_id: &str,
    reusable_on: bool,
    now: SystemTime,
) -> Result<(String, Vec<DeviceId>), Status> {
    // device-identity: Reusable review admin invite. Only a server with the
    // review flag honours a reusable row; without it the row is unknown below.
    if reusable_on
        && let Some(username) = crate::admin::invites::reusable_username(tx, invite_code)?
    {
        return take_over_admin(tx, &username, identity_public_key, signed, server_id, now);
    }
    let invite: Option<(String, i64, Option<i64>, Option<i64>)> = tx
        .query_row(
            "SELECT username, expires_at, used_at, revoked_at FROM invites
             WHERE code = ?1 AND reusable = 0",
            params![invite_code],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .optional()
        .map_err(StoreError::from)?;
    let username = match invite {
        Some((username, expires_at, None, None)) if now < from_unix_millis(expires_at) => username,
        // group-resync: an otherwise-valid invite that has only expired gets a
        // distinct status, so the app can say "this invite has expired — ask
        // for a fresh one" instead of a generic failure (app-ui: Invite expiry
        // is surfaced). An invite is valid for `invite_ttl` (7 days default).
        Some((_, _, None, None)) => {
            return Err(Status::deadline_exceeded(
                "the invite has expired; ask for a fresh one",
            ));
        }
        _ => {
            return Err(Status::permission_denied(
                "the invite is unknown, used or revoked",
            ));
        }
    };
    // Forged certificate: PERMISSION_DENIED (User identity key and device
    // certificates); malformed: INVALID_ARGUMENT.
    let cert = certificate::verify(signed, identity_public_key)?;
    if cert.username != username || cert.server_id != server_id {
        return Err(Status::permission_denied(format!(
            "the certificate must name {username}@{server_id}, the user the invite creates"
        )));
    }
    let mut revoked = Vec::new();
    match records::user(tx, &username)? {
        None => {
            tx.execute(
                "INSERT INTO users (username, identity_public_key, created_at) VALUES (?1, ?2, ?3)",
                params![username, identity_public_key, unix_millis(now)],
            )
            .map_err(StoreError::from)?;
        }
        // The admin reset a removed user (group-membership: Removing a user):
        // the new signup brings the new identity key. The old devices stay
        // revoked.
        Some(user) if user.removed => {
            tx.execute(
                "UPDATE users SET identity_public_key = ?2, created_at = ?3, removed_at = NULL
                 WHERE username = ?1",
                params![username, identity_public_key, unix_millis(now)],
            )
            .map_err(StoreError::from)?;
        }
        // device-identity: Re-inviting an existing user. A valid invite whose
        // bound username is a present, non-removed user is a re-key: the new
        // signup brings a new identity key for the SAME user id, and every
        // prior device is revoked (a re-invite is a lost-device recovery; two
        // live identity keys for one user must never coexist). The user row is
        // kept (same user_id); only the identity key changes. Members see the
        // changed key and re-trust (key-directory: Clients verify the identity
        // chain).
        Some(_) => {
            tx.execute(
                "UPDATE users SET identity_public_key = ?2 WHERE username = ?1",
                params![username, identity_public_key],
            )
            .map_err(StoreError::from)?;
            for device in records::active_devices(tx, &username)? {
                if auth::revoke_device(tx, &device.device_id, now)? {
                    revoked.push(device.device_id);
                }
            }
        }
    }
    insert_device(tx, &cert, now)?;
    tx.execute(
        "UPDATE invites SET used_at = ?2 WHERE code = ?1",
        params![invite_code, unix_millis(now)],
    )
    .map_err(StoreError::from)?;
    Ok((username, revoked))
}

/// device-identity: Reusable review admin invite. The newest registration with
/// the review server's reusable invite takes over the admin seat in this one
/// transaction: every active admin device and the admin's single-use invites
/// are revoked and the lobby is re-seeded (the steps `server admin-reset`
/// takes), then the admin's identity key becomes the new one and the device is
/// registered. The invite is never marked used. Returns the revoked devices so
/// the caller closes their streams.
fn take_over_admin(
    tx: &Transaction<'_>,
    username: &str,
    identity_public_key: &[u8],
    signed: &pb::SignedDeviceCertificate,
    server_id: &str,
    now: SystemTime,
) -> Result<(String, Vec<DeviceId>), Status> {
    // Check the certificate before touching anything, so a bad request leaves
    // the current reviewer in place.
    let cert = certificate::verify(signed, identity_public_key)?;
    if cert.username != username || cert.server_id != server_id {
        return Err(Status::permission_denied(format!(
            "the certificate must name {username}@{server_id}, the user the invite creates"
        )));
    }
    let cleared = crate::runtime::admin_reset::clear_admin_seat(tx, username, now)?;
    match records::user(tx, username)? {
        None => {
            tx.execute(
                "INSERT INTO users (username, identity_public_key, created_at) VALUES (?1, ?2, ?3)",
                params![username, identity_public_key, unix_millis(now)],
            )
            .map_err(StoreError::from)?;
        }
        Some(_) => {
            tx.execute(
                "UPDATE users SET identity_public_key = ?2, created_at = ?3, removed_at = NULL
                 WHERE username = ?1",
                params![username, identity_public_key, unix_millis(now)],
            )
            .map_err(StoreError::from)?;
        }
    }
    insert_device(tx, &cert, now)?;
    Ok((username.to_owned(), cleared.revoked))
}

/// device-identity: Device linking. Returns the new device's id.
fn link(
    tx: &Transaction<'_>,
    username: &str,
    server_id: &str,
    signed: &pb::SignedDeviceCertificate,
    now: SystemTime,
) -> Result<DeviceId, Status> {
    let user = records::user(tx, username)?
        .ok_or_else(|| Status::unauthenticated("caller's user not found"))?;
    let cert = certificate::verify(signed, &user.identity_public_key)?;
    if cert.username != username || cert.server_id != server_id {
        return Err(Status::permission_denied(
            "the certificate must name the caller's own user",
        ));
    }
    insert_device(tx, &cert, now)?;
    Ok(cert.device_id)
}
