//! Devices service: server info, signup, challenge and authentication,
//! linking, revocation and push tokens (pi-server-core tasks 4.1-4.3).

use std::time::SystemTime;

use protocol::device_identity::DeviceId;
use protocol::wire_transport::{
    CHALLENGE_NONCE_LEN, CHALLENGE_TTL, MAX_APPLICATION_CIPHERTEXT_BYTES,
    MAX_HANDSHAKE_ENVELOPE_BYTES, PROTOCOL_VERSION, verify_auth_signature,
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
                supported_protocol_versions: vec![PROTOCOL_VERSION],
                limits: Some(pb::Limits {
                    max_application_ciphertext_bytes: as_u32(MAX_APPLICATION_CIPHERTEXT_BYTES),
                    max_handshake_envelope_bytes: as_u32(MAX_HANDSHAKE_ENVELOPE_BYTES),
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
        let now = self.state.clock.now();
        let username = self
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
                    now,
                );
                if result.is_ok() {
                    tx.commit()?;
                }
                Ok(result)
            })
            .await??;
        tracing::info!(%username, "registered a user and their first device");
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
        let expires_at = now + CHALLENGE_TTL;
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
        tracing::info!(device_id = %crate::runtime::server_keys::hex(&device_id), "linked a device");
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
        tracing::info!(device_id = %crate::runtime::server_keys::hex(&target), "revoked a device");
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
                if token.token.is_empty() {
                    return Err(Status::invalid_argument("push_token.token is empty"));
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
}

/// device-identity: Signup with an invite; Admin-owned usernames. Returns the
/// new user's username.
fn register(
    tx: &Transaction<'_>,
    invite_code: &str,
    identity_public_key: &[u8],
    signed: &pb::SignedDeviceCertificate,
    server_id: &str,
    now: SystemTime,
) -> Result<String, Status> {
    let invite: Option<(String, i64, Option<i64>, Option<i64>)> = tx
        .query_row(
            "SELECT username, expires_at, used_at, revoked_at FROM invites WHERE code = ?1",
            params![invite_code],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .optional()
        .map_err(StoreError::from)?;
    let username = match invite {
        Some((username, expires_at, None, None)) if now < from_unix_millis(expires_at) => username,
        _ => {
            return Err(Status::permission_denied(
                "the invite is unknown, used, revoked or expired",
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
        Some(_) => return Err(Status::already_exists("this user already exists")),
    }
    insert_device(tx, &cert, now)?;
    tx.execute(
        "UPDATE invites SET used_at = ?2 WHERE code = ?1",
        params![invite_code, unix_millis(now)],
    )
    .map_err(StoreError::from)?;
    Ok(username)
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
