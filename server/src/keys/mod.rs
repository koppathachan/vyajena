//! Keys service: the KeyPackage directory and identity keys (key-directory:
//! KeyPackage upload; KeyPackage fetch; Identity key lookup; server-core
//! task 5.1, client-core task 5.5).

use std::time::SystemTime;

use openmls::prelude::tls_codec::Deserialize as _;
use openmls::prelude::{ExtensionType, KeyPackageIn, ProtocolVersion};
use openmls_rust_crypto::RustCrypto;
use protocol::device_identity::DeviceId;
use protocol::group_membership::{
    ADMIN_EXTENSION_TYPE, DM_EXTENSION_TYPE, MAX_READD_REASON_BYTES, NAMED_EXTENSION_TYPE,
};
use protocol::key_directory::KEY_PACKAGES_LOW_WATER_MARK;
use rusqlite::{OptionalExtension, Transaction, params};
use tonic::{Request, Response, Status};

use crate::auth::caller;
use crate::membership::{CIPHERSUITE, certificate, group, inactivity, joins, readd};
use crate::messaging;
use crate::pb;
use crate::pb::keys_server::{Keys, KeysServer};
use crate::runtime::clock::unix_millis;
use crate::runtime::{KeyPackagesLow, State};
use crate::store::StoreError;
use crate::store::records::{self, DeviceRow};

/// The gRPC service.
pub fn service(state: &State) -> KeysServer<KeysService> {
    KeysServer::new(KeysService {
        state: state.clone(),
    })
}

/// Implements `vyajena.v1.Keys`.
pub struct KeysService {
    state: State,
}

/// Checks one uploaded KeyPackage against the uploading device
/// (key-directory: KeyPackage upload): it parses and verifies, uses the one
/// ciphersuite, its credential carries exactly the device's registered
/// certificate, its leaf signature key is the certificate's MLS key, and its
/// capabilities list [`ADMIN_EXTENSION_TYPE`]. Returns whether they also list
/// [`DM_EXTENSION_TYPE`] (direct-messages: Old apps never join a DM) and
/// [`NAMED_EXTENSION_TYPE`] (multi-group: Named groups reach only v3 apps).
fn check_key_package(
    crypto: &RustCrypto,
    bytes: &[u8],
    device: &DeviceRow,
) -> Result<Caps, Status> {
    let key_package = KeyPackageIn::tls_deserialize_exact(bytes)
        .map_err(|_| Status::invalid_argument("a KeyPackage does not parse"))?
        .validate(crypto, ProtocolVersion::Mls10)
        .map_err(|e| Status::invalid_argument(format!("a KeyPackage is invalid: {e}")))?;
    if key_package.ciphersuite() != CIPHERSUITE {
        return Err(Status::invalid_argument(
            "KeyPackages must use MLS_128_DHKEMX25519_AES128GCM_SHA256_Ed25519",
        ));
    }
    let leaf = key_package.leaf_node();
    // Without the admin group-context extension in its capabilities the
    // device could never be added (server-core design D10).
    if !leaf
        .capabilities()
        .extensions()
        .contains(&ExtensionType::Unknown(ADMIN_EXTENSION_TYPE))
    {
        return Err(Status::invalid_argument(
            "a KeyPackage's capabilities do not list the admin group-context extension 0xF0A1",
        ));
    }
    let carries_own_certificate =
        certificate::from_credential(leaf.credential()).is_some_and(|signed| {
            signed.certificate == device.certificate
                && signed.signature == device.certificate_signature
        });
    if !carries_own_certificate {
        return Err(Status::permission_denied(
            "a KeyPackage's credential is not the uploading device's certificate",
        ));
    }
    if leaf.signature_key().as_slice() != device.mls_signature_public_key.as_slice() {
        return Err(Status::permission_denied(
            "a KeyPackage's signature key is not the certificate's MLS key",
        ));
    }
    let extensions = leaf.capabilities().extensions();
    Ok(Caps {
        dm: extensions.contains(&ExtensionType::Unknown(DM_EXTENSION_TYPE)),
        named: extensions.contains(&ExtensionType::Unknown(NAMED_EXTENSION_TYPE)),
    })
}

/// The optional group-context extensions a KeyPackage lists.
#[derive(Debug, Clone, Copy)]
struct Caps {
    dm: bool,
    named: bool,
}

/// Which capability a fetch for a group needs (key-directory: KeyPackage fetch).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i64)]
enum Needs {
    Nothing = 0,
    Dm = 1,
    Named = 2,
}

/// Checks every KeyPackage of an upload; one bad one fails it all. Returns
/// the capabilities of each one-time KeyPackage and of the last-resort one.
fn check_upload(
    crypto: &RustCrypto,
    one_time: &[Vec<u8>],
    last_resort: &[u8],
    device: &DeviceRow,
) -> Result<(Vec<Caps>, Caps), Status> {
    let mut flags = Vec::with_capacity(one_time.len());
    for bytes in one_time {
        flags.push(check_key_package(crypto, bytes, device)?);
    }
    Ok((flags, check_key_package(crypto, last_resort, device)?))
}

fn store_key_packages(
    tx: &Transaction<'_>,
    device_id: &DeviceId,
    one_time: &[Vec<u8>],
    last_resort: &[u8],
    flags: &(Vec<Caps>, Caps),
    now: SystemTime,
) -> Result<(), StoreError> {
    let mut insert = tx.prepare(
        "INSERT INTO key_packages (device_id, key_package, last_resort, uploaded_at, dm_capable,
                                   named_group_capable)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
    )?;
    for (key_package, caps) in one_time.iter().zip(&flags.0) {
        insert.execute(params![
            device_id.as_slice(),
            key_package,
            false,
            unix_millis(now),
            caps.dm,
            caps.named
        ])?;
    }
    // The new last-resort KeyPackage replaces the old one.
    tx.execute(
        "DELETE FROM key_packages WHERE device_id = ?1 AND last_resort = 1",
        params![device_id.as_slice()],
    )?;
    insert.execute(params![
        device_id.as_slice(),
        last_resort,
        true,
        unix_millis(now),
        flags.1.dm,
        flags.1.named
    ])?;
    Ok(())
}

/// Takes one KeyPackage of `device_id`: the oldest one-time one, which is
/// deleted, or else the last-resort one, which stays. With `needs`, only one
/// that lists the extension of a DM or named group (key-directory: KeyPackage
/// fetch).
fn take_key_package(
    tx: &Transaction<'_>,
    device_id: &DeviceId,
    needs: Needs,
) -> Result<Option<Vec<u8>>, StoreError> {
    let one_time: Option<Vec<u8>> = tx
        .query_row(
            "DELETE FROM key_packages WHERE id = (
                 SELECT id FROM key_packages WHERE device_id = ?1 AND last_resort = 0
                   AND (?2 = 0 OR (?2 = 1 AND dm_capable = 1) OR (?2 = 2 AND named_group_capable = 1))
                 ORDER BY id LIMIT 1)
             RETURNING key_package",
            params![device_id.as_slice(), needs as i64],
            |row| row.get(0),
        )
        .optional()?;
    if one_time.is_some() {
        return Ok(one_time);
    }
    Ok(tx
        .query_row(
            "SELECT key_package FROM key_packages WHERE device_id = ?1 AND last_resort = 1
               AND (?2 = 0 OR (?2 = 1 AND dm_capable = 1) OR (?2 = 2 AND named_group_capable = 1))",
            params![device_id.as_slice(), needs as i64],
            |row| row.get(0),
        )
        .optional()?)
}

#[tonic::async_trait]
impl Keys for KeysService {
    async fn upload_key_packages(
        &self,
        request: Request<pb::UploadKeyPackagesRequest>,
    ) -> Result<Response<pb::UploadKeyPackagesResponse>, Status> {
        let caller = caller(&request)?.clone();
        let request = request.into_inner();
        if request.last_resort_key_package.is_empty() {
            return Err(Status::invalid_argument(
                "exactly one last-resort KeyPackage is required",
            ));
        }
        let now = self.state.clock.now();
        let count = request.key_packages.len();
        let admin = self.state.config.admin_username.clone();
        let server_id = self.state.config.server_id.clone();
        let announce = self
            .state
            .store
            .call(move |conn| {
                let tx = conn.transaction()?;
                let device = records::device(&tx, &caller.device_id)?
                    .ok_or_else(|| StoreError::Inconsistent("caller's device is missing".into()))?;
                let crypto = RustCrypto::default();
                // One bad KeyPackage fails the whole upload.
                let flags = match check_upload(
                    &crypto,
                    &request.key_packages,
                    &request.last_resort_key_package,
                    &device,
                ) {
                    Ok(flags) => flags,
                    Err(status) => return Ok(Err(status)),
                };
                store_key_packages(
                    &tx,
                    &caller.device_id,
                    &request.key_packages,
                    &request.last_resort_key_package,
                    &flags,
                    now,
                )?;
                // group-membership: Automatic add of new users;
                // direct-messages: DM members kept complete.
                let announce = joins::after_upload(&tx, &caller.device_id, &admin, &server_id)?;
                tx.commit()?;
                Ok(Ok(announce))
            })
            .await??;
        tracing::debug!(one_time = count, "stored KeyPackages");
        if !announce.is_empty() {
            tracing::info!("a new device waits to be added; telling the admin's devices");
        }
        for (helper, notice) in announce {
            self.state.hub.send_frame(&helper, notice);
        }
        Ok(Response::new(pb::UploadKeyPackagesResponse {}))
    }

    // group-membership: Re-add after an unusable Welcome (welcome-recovery
    // design D2-D4).
    async fn request_readd(
        &self,
        request: Request<pb::RequestReaddRequest>,
    ) -> Result<Response<pb::RequestReaddResponse>, Status> {
        let caller = caller(&request)?.clone();
        let request = request.into_inner();
        if request.last_resort_key_package.is_empty() {
            return Err(Status::invalid_argument(
                "exactly one last-resort KeyPackage is required",
            ));
        }
        if request.reason.len() > MAX_READD_REASON_BYTES {
            return Err(Status::invalid_argument(format!(
                "the reason is over {MAX_READD_REASON_BYTES} bytes"
            )));
        }
        let now = self.state.clock.now();
        let device = crate::runtime::redact(&caller.device_id);
        let limited = !self.state.readd_asks.allowed(&caller.device_id, now);
        let admin = self.state.config.admin_username.clone();
        let server_id = self.state.config.server_id.clone();
        let keys = self.state.keys.clone();
        let limits = self.state.config.limits;
        let refused_seq = request.refused_welcome_seq;
        let reason = request.reason.clone();
        let device_id = caller.device_id;
        let decided = self
            .state
            .store
            .call(move |conn| {
                let tx = conn.transaction()?;
                let device = records::device(&tx, &caller.device_id)?
                    .ok_or_else(|| StoreError::Inconsistent("caller's device is missing".into()))?;
                let crypto = RustCrypto::default();
                let flags = match check_upload(
                    &crypto,
                    &request.key_packages,
                    &request.last_resort_key_package,
                    &device,
                ) {
                    Ok(flags) => flags,
                    Err(status) => return Ok(Err(status)),
                };
                let outcome = if !readd::is_member(&tx, &caller.device_id)? {
                    pb::ReaddOutcome::NotAMember
                } else if inactivity::is_inactive(&tx, &caller.device_id)? {
                    pb::ReaddOutcome::AlreadyWaiting
                } else if readd::welcome_queued(&tx, &caller.device_id, refused_seq)? {
                    pb::ReaddOutcome::WelcomeQueued
                } else if limited {
                    pb::ReaddOutcome::LimitReached
                } else {
                    pb::ReaddOutcome::Asked
                };
                let Some((group_id, _)) =
                    messaging::group_row(&tx)?.filter(|_| outcome == pb::ReaddOutcome::Asked)
                else {
                    return Ok(Ok((outcome, Vec::new(), Vec::new())));
                };
                tx.execute(
                    "DELETE FROM key_packages WHERE device_id = ?1",
                    params![caller.device_id.as_slice()],
                )?;
                store_key_packages(
                    &tx,
                    &caller.device_id,
                    &request.key_packages,
                    &request.last_resort_key_package,
                    &flags,
                    now,
                )?;
                let ctx = messaging::Context {
                    server_id: &server_id,
                    keys: &keys,
                    now,
                    limits,
                };
                let notify = messaging::mark_inactive(&tx, &ctx, &caller.device_id)?;
                inactivity::mark_returned(&tx, &caller.device_id, now)?;
                let me = messaging::device_ref(&caller.device_id, &caller.username, &server_id);
                let rejoin = inactivity::active_members(&tx, &group_id)?
                    .into_iter()
                    .filter(|(helper, helper_user)| {
                        messaging::can_readd(
                            helper,
                            helper_user,
                            &caller.device_id,
                            &caller.username,
                            &admin,
                        )
                    })
                    .map(|(helper, _)| (helper, messaging::rejoin_notice(&group_id, me.clone())))
                    .collect::<Vec<_>>();
                tx.commit()?;
                Ok(Ok((outcome, notify, rejoin)))
            })
            .await??;
        let (outcome, notify, rejoin) = decided;
        if outcome == pb::ReaddOutcome::Asked {
            self.state.readd_asks.record(&device_id, now);
        }
        // The core's error text (never content), so the owner can read the
        // cause in the server's journal.
        tracing::info!(
            device = %device,
            refused_welcome_seq = refused_seq,
            outcome = outcome.as_str_name(),
            reason = %reason,
            "a device asked to be added again"
        );
        messaging::notify(&self.state, notify, crate::push::PingClass::Nudge);
        for (helper, notice) in rejoin {
            self.state.hub.send_frame(&helper, notice);
        }
        Ok(Response::new(pb::RequestReaddResponse {
            outcome: outcome.into(),
        }))
    }

    async fn fetch_key_packages(
        &self,
        request: Request<pb::FetchKeyPackagesRequest>,
    ) -> Result<Response<pb::FetchKeyPackagesResponse>, Status> {
        let fetcher = caller(&request)?.device_id;
        let request = request.into_inner();
        let dm_group_id = request.group_id.clone();
        let user_id = request
            .user_id
            .ok_or_else(|| Status::invalid_argument("user_id is required"))?;
        if user_id.server_id != self.state.config.server_id {
            return Err(Status::not_found("unknown user"));
        }
        let mut wanted = Vec::with_capacity(request.device_ids.len());
        for id in &request.device_ids {
            let id = records::device_id_from(id)
                .ok_or_else(|| Status::invalid_argument("device ids are 16 bytes"))?;
            if !wanted.contains(&id) {
                wanted.push(id);
            }
        }
        let response = self
            .state
            .store
            .call(move |conn| {
                let tx = conn.transaction()?;
                let Some(user) = records::user(&tx, &user_id.username)? else {
                    return Ok(Err(Status::not_found("unknown user")));
                };
                // key-directory: KeyPackage fetch, for a DM (change `dms`).
                let mut needs = Needs::Nothing;
                if !dm_group_id.is_empty() {
                    match group::row(&tx, &dm_group_id)? {
                        Some(group::GroupRow {
                            group_id,
                            kind: group::GroupKind::Named { closed_at: None },
                            ..
                        }) => {
                            needs = Needs::Named;
                            let member: bool = tx.query_row(
                                "SELECT EXISTS (SELECT 1 FROM mls_members
                                                WHERE group_id = ?1 AND device_id = ?2)",
                                params![group_id, fetcher.as_slice()],
                                |row| row.get(0),
                            )?;
                            if !member {
                                return Ok(Err(Status::permission_denied(
                                    "only a member of this group fetches for it",
                                )));
                            }
                        }
                        Some(group::GroupRow {
                            group_id,
                            kind:
                                group::GroupKind::Dm {
                                    users,
                                    closed_at: None,
                                },
                            ..
                        }) => {
                            let member: bool = tx.query_row(
                                "SELECT EXISTS (SELECT 1 FROM mls_members
                                                WHERE group_id = ?1 AND device_id = ?2)",
                                params![group_id, fetcher.as_slice()],
                                |row| row.get(0),
                            )?;
                            needs = Needs::Dm;
                            if !member || (users.0 != user.username && users.1 != user.username) {
                                return Ok(Err(Status::permission_denied(
                                    "only a member of this DM fetches for its two users",
                                )));
                            }
                        }
                        Some(_) => {
                            return Ok(Err(Status::permission_denied(
                                "group_id does not name an open DM or named group",
                            )));
                        }
                        None => return Ok(Err(Status::not_found("unknown group"))),
                    }
                }
                let mut devices = records::active_devices(&tx, &user.username)?;
                if !wanted.is_empty() {
                    if wanted
                        .iter()
                        .any(|id| !devices.iter().any(|d| d.device_id == *id))
                    {
                        return Ok(Err(Status::not_found(
                            "a listed device is not a non-revoked device of this user",
                        )));
                    }
                    devices.retain(|d| wanted.contains(&d.device_id));
                }
                let mut entries = Vec::with_capacity(devices.len());
                let mut low = Vec::new();
                for device in devices {
                    let before = records::one_time_key_packages(&tx, &device.device_id)?;
                    // A device that never uploaded KeyPackages cannot be added
                    // yet and is left out.
                    if let Some(key_package) = take_key_package(&tx, &device.device_id, needs)? {
                        let remaining = before.saturating_sub(1);
                        if before > 0 && (remaining as usize) < KEY_PACKAGES_LOW_WATER_MARK {
                            low.push(KeyPackagesLow {
                                device_id: device.device_id,
                                remaining,
                            });
                        }
                        entries.push(pb::DeviceKeyPackage {
                            certificate: Some(pb::SignedDeviceCertificate {
                                certificate: device.certificate,
                                signature: device.certificate_signature,
                            }),
                            key_package,
                        });
                    }
                }
                tx.commit()?;
                Ok(Ok((
                    pb::FetchKeyPackagesResponse {
                        identity_public_key: user.identity_public_key,
                        devices: entries,
                    },
                    low,
                )))
            })
            .await??;
        let (response, low) = response;
        // key-directory: Replenishment signal; the messaging service tells
        // the device if it is connected.
        for count in low {
            let _ = self.state.key_packages_low.send(count);
        }
        Ok(Response::new(response))
    }

    /// key-directory: Identity key lookup. All or nothing: one unknown user,
    /// or one without a non-revoked device, fails the call with `NOT_FOUND`.
    async fn get_identity_keys(
        &self,
        request: Request<pb::GetIdentityKeysRequest>,
    ) -> Result<Response<pb::GetIdentityKeysResponse>, Status> {
        caller(&request)?;
        let mut wanted: Vec<pb::UserId> = Vec::new();
        for user_id in request.into_inner().user_ids {
            if user_id.server_id != self.state.config.server_id {
                return Err(Status::not_found("unknown user"));
            }
            if !wanted.contains(&user_id) {
                wanted.push(user_id);
            }
        }
        let keys = self
            .state
            .store
            .call(move |conn| {
                let mut keys = Vec::with_capacity(wanted.len());
                for user_id in wanted {
                    let Some(user) = records::user(conn, &user_id.username)? else {
                        return Ok(Err(Status::not_found("unknown user")));
                    };
                    if records::active_devices(conn, &user.username)?.is_empty() {
                        return Ok(Err(Status::not_found(
                            "a listed user has no non-revoked device",
                        )));
                    }
                    keys.push(pb::UserIdentityKey {
                        user_id: Some(user_id),
                        identity_public_key: user.identity_public_key,
                    });
                }
                Ok(Ok(keys))
            })
            .await??;
        Ok(Response::new(pb::GetIdentityKeysResponse { keys }))
    }
}
