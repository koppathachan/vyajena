//! Keys service: the KeyPackage directory and identity keys (key-directory:
//! KeyPackage upload; KeyPackage fetch; Identity key lookup; pi-server-core
//! task 5.1, client-core task 5.5).

use std::time::SystemTime;

use openmls::prelude::tls_codec::Deserialize as _;
use openmls::prelude::{ExtensionType, KeyPackageIn, ProtocolVersion};
use openmls_rust_crypto::RustCrypto;
use protocol::device_identity::DeviceId;
use protocol::group_membership::ADMIN_EXTENSION_TYPE;
use protocol::key_directory::KEY_PACKAGES_LOW_WATER_MARK;
use rusqlite::{OptionalExtension, Transaction, params};
use tonic::{Request, Response, Status};

use crate::auth::caller;
use crate::membership::{CIPHERSUITE, certificate, joins};
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
/// capabilities list [`ADMIN_EXTENSION_TYPE`].
fn check_key_package(crypto: &RustCrypto, bytes: &[u8], device: &DeviceRow) -> Result<(), Status> {
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
    // device could never be added (pi-server-core design D10).
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
    Ok(())
}

fn store_key_packages(
    tx: &Transaction<'_>,
    device_id: &DeviceId,
    one_time: &[Vec<u8>],
    last_resort: &[u8],
    now: SystemTime,
) -> Result<(), StoreError> {
    let mut insert = tx.prepare(
        "INSERT INTO key_packages (device_id, key_package, last_resort, uploaded_at)
         VALUES (?1, ?2, ?3, ?4)",
    )?;
    for key_package in one_time {
        insert.execute(params![
            device_id.as_slice(),
            key_package,
            false,
            unix_millis(now)
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
        unix_millis(now)
    ])?;
    Ok(())
}

/// Takes one KeyPackage of `device_id`: the oldest one-time one, which is
/// deleted, or else the last-resort one, which stays.
fn take_key_package(
    tx: &Transaction<'_>,
    device_id: &DeviceId,
) -> Result<Option<Vec<u8>>, StoreError> {
    let one_time: Option<Vec<u8>> = tx
        .query_row(
            "DELETE FROM key_packages WHERE id = (
                 SELECT id FROM key_packages WHERE device_id = ?1 AND last_resort = 0
                 ORDER BY id LIMIT 1)
             RETURNING key_package",
            params![device_id.as_slice()],
            |row| row.get(0),
        )
        .optional()?;
    if one_time.is_some() {
        return Ok(one_time);
    }
    Ok(tx
        .query_row(
            "SELECT key_package FROM key_packages WHERE device_id = ?1 AND last_resort = 1",
            params![device_id.as_slice()],
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
                for bytes in request
                    .key_packages
                    .iter()
                    .chain(std::iter::once(&request.last_resort_key_package))
                {
                    if let Err(status) = check_key_package(&crypto, bytes, &device) {
                        return Ok(Err(status));
                    }
                }
                store_key_packages(
                    &tx,
                    &caller.device_id,
                    &request.key_packages,
                    &request.last_resort_key_package,
                    now,
                )?;
                // group-membership: Automatic add of new users.
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

    async fn fetch_key_packages(
        &self,
        request: Request<pb::FetchKeyPackagesRequest>,
    ) -> Result<Response<pb::FetchKeyPackagesResponse>, Status> {
        caller(&request)?;
        let request = request.into_inner();
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
                    if let Some(key_package) = take_key_package(&tx, &device.device_id)? {
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
