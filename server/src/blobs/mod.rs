//! Blobs service: encrypted image blobs (blob-store; change `media-blobs`).
//!
//! A blob is opaque XChaCha20-Poly1305 ciphertext made on the sending device;
//! its key travels only inside an MLS message, so the server never holds it
//! (blob-store: Blobs are opaque ciphertext). The bytes live in the configured
//! [`BlobStore`] backend; the metadata, who may read a blob and when it dies
//! live in the server's SQLite ([`meta`]), so a backend can be swapped without
//! touching either (blob-store: Pluggable storage backend).
//!
//! Upload and download are checked against the caller's membership of the
//! blob's group, so any group or DM works the same way. With no backend, or one
//! that is unavailable, every RPC answers `UNAVAILABLE` and messaging goes on
//! (blob-store: Blob storage and graceful degradation).

pub mod backend;
pub mod meta;

use std::sync::Arc;
use std::time::Duration;

use protocol::blob_store::{
    BLOB_ID_LEN, MAX_BLOB_BYTES, MAX_CONCURRENT_UPLOADS, MAX_UPLOAD_REQUEST_BYTES,
};
use sha2::{Digest, Sha256};
use tokio::task::JoinHandle;
use tonic::{Request, Response, Status};

use crate::auth::caller;
use crate::membership::GroupState;
use crate::pb;
use crate::pb::blobs_server::{Blobs, BlobsServer};
use crate::runtime::clock::{from_unix_millis, to_timestamp, unix_millis};
use crate::runtime::config::BlobStoreConfig;
use crate::runtime::{Config, State};
use crate::store::StoreError;

pub use backend::{BackendError, BlobId, BlobStore, FilesystemStore};
use meta::BlobRow;

/// The backend `config` names, or `None` when the blob store is not
/// configured (blob-store: Blob storage and graceful degradation).
pub fn backend_for(config: &Config) -> Option<Arc<dyn BlobStore>> {
    match config.blob_store.as_ref()? {
        BlobStoreConfig::Filesystem { path } => Some(Arc::new(FilesystemStore::new(path.clone()))),
    }
}

/// The gRPC service.
pub fn service(state: &State) -> BlobsServer<BlobsService> {
    BlobsServer::new(BlobsService {
        state: state.clone(),
        uploads: Arc::new(tokio::sync::Semaphore::new(MAX_CONCURRENT_UPLOADS)),
    })
    // A request over the blob and its ids is refused before it is buffered
    // (blob-store: Upload limits).
    .max_decoding_message_size(MAX_UPLOAD_REQUEST_BYTES)
}

/// Implements `vyajena.v1.Blobs`.
pub struct BlobsService {
    state: State,
    /// One permit per upload in progress (blob-store: Upload limits).
    uploads: Arc<tokio::sync::Semaphore>,
}

/// A backend failure seen by a client: logged, reported as `UNAVAILABLE`
/// without the path or address in it.
fn unavailable(error: BackendError) -> Status {
    tracing::warn!(%error, "blob store unavailable");
    Status::unavailable("the blob store is unavailable")
}

fn blob_id(bytes: &[u8]) -> Result<BlobId, Status> {
    BlobId::try_from(bytes)
        .map_err(|_| Status::invalid_argument(format!("a blob id is {BLOB_ID_LEN} bytes")))
}

/// Whether `device` is a current member device of `group_id`.
fn is_member(
    conn: &rusqlite::Connection,
    group_id: &[u8],
    device: &[u8],
) -> Result<bool, StoreError> {
    Ok(GroupState::members(conn, group_id)?
        .iter()
        .any(|member| member.device_id.as_slice() == device))
}

fn not_a_member() -> Status {
    Status::permission_denied("this device is not a member of that group")
}

fn ttl_millis(blob_ttl: Duration) -> i64 {
    i64::try_from(blob_ttl.as_millis()).unwrap_or(i64::MAX)
}

/// What an upload does after the first look at the database.
enum Decision {
    /// The same bytes were uploaded before; answer with the same expiry.
    Existing(i64),
    /// A new blob: store the bytes, then record the row.
    Store,
    /// A new blob, refused by an upload limit (blob-store: Upload limits).
    Limited(&'static str),
}

impl BlobsService {
    /// The backend, checked usable. Every RPC starts here.
    async fn backend(&self) -> Result<Arc<dyn BlobStore>, Status> {
        let backend = self
            .state
            .blobs
            .clone()
            .ok_or_else(|| Status::unavailable("this server does not store images"))?;
        backend.check().await.map_err(unavailable)?;
        Ok(backend)
    }
}

#[tonic::async_trait]
impl Blobs for BlobsService {
    // blob-store: Upload.
    async fn upload_blob(
        &self,
        request: Request<pb::UploadBlobRequest>,
    ) -> Result<Response<pb::UploadBlobResponse>, Status> {
        let caller = caller(&request)?.clone();
        let request = request.into_inner();
        let backend = self.backend().await?;
        let id = blob_id(&request.blob_id)?;
        if request.ciphertext.is_empty() {
            return Err(Status::invalid_argument("a blob is not empty"));
        }
        if request.ciphertext.len() > MAX_BLOB_BYTES {
            return Err(Status::invalid_argument(format!(
                "a blob is at most {MAX_BLOB_BYTES} bytes"
            )));
        }
        // At most MAX_CONCURRENT_UPLOADS at once: each is buffered whole.
        let _permit = self
            .uploads
            .clone()
            .try_acquire_owned()
            .map_err(|_| Status::resource_exhausted("too many uploads at once; try again"))?;
        let now_ms = unix_millis(self.state.clock.now());
        let row = BlobRow {
            blob_id: id.to_vec(),
            group_id: request.group_id.clone(),
            uploader_device_id: caller.device_id.to_vec(),
            size: i64::try_from(request.ciphertext.len()).unwrap_or(i64::MAX),
            sha256: Sha256::digest(&request.ciphertext).to_vec(),
            uploaded_at: now_ms,
            expires_at: now_ms.saturating_add(ttl_millis(self.state.config.limits.blob_ttl)),
        };

        let size = row.size;
        let first = row.clone();
        let max_blob_store_bytes = self.state.config.limits.max_blob_store_bytes;
        let max_uploads_per_hour = self.state.config.limits.max_uploads_per_device_per_hour;
        let decision = self
            .state
            .store
            .call(move |conn| {
                if !is_member(conn, &first.group_id, &first.uploader_device_id)? {
                    return Ok(Err(not_a_member()));
                }
                Ok(match meta::find(conn, &first.blob_id)? {
                    Some(old) if !old.expired(now_ms) => {
                        if old.group_id == first.group_id && old.sha256 == first.sha256 {
                            Ok(Decision::Existing(old.expires_at))
                        } else {
                            Err(Status::already_exists("that blob id is taken"))
                        }
                    }
                    _ => {
                        // Upload limits: the live total and this device's hour.
                        let hour_ago = now_ms.saturating_sub(3_600_000);
                        if meta::live_bytes(conn, now_ms)?.saturating_add(first.size.unsigned_abs())
                            > max_blob_store_bytes
                        {
                            Ok(Decision::Limited("the blob store is full"))
                        } else if meta::uploads_since(conn, &first.uploader_device_id, hour_ago)?
                            >= max_uploads_per_hour
                        {
                            Ok(Decision::Limited("too many uploads from this device"))
                        } else {
                            Ok(Decision::Store)
                        }
                    }
                })
            })
            .await??;
        let expires_at = match decision {
            Decision::Limited(why) => return Err(Status::resource_exhausted(why)),
            Decision::Existing(expires_at) => expires_at,
            Decision::Store => {
                backend
                    .put(&id, request.ciphertext)
                    .await
                    .map_err(unavailable)?;
                self.state
                    .store
                    .call(move |conn| {
                        let tx = conn.transaction()?;
                        if let Some(old) = meta::find(&tx, &row.blob_id)? {
                            if !old.expired(now_ms) {
                                // Another upload of this id got in first.
                                return Ok(
                                    if old.group_id == row.group_id && old.sha256 == row.sha256 {
                                        Ok(old.expires_at)
                                    } else {
                                        Err(Status::already_exists("that blob id is taken"))
                                    },
                                );
                            }
                            meta::delete(&tx, &row.blob_id)?;
                        }
                        meta::insert(&tx, &row)?;
                        tx.commit()?;
                        Ok(Ok(row.expires_at))
                    })
                    .await??
            }
        };
        tracing::debug!(size, "handled a blob upload");
        Ok(Response::new(pb::UploadBlobResponse {
            expires_at: Some(to_timestamp(from_unix_millis(expires_at))),
        }))
    }

    // blob-store: Download.
    async fn download_blob(
        &self,
        request: Request<pb::DownloadBlobRequest>,
    ) -> Result<Response<pb::DownloadBlobResponse>, Status> {
        let caller = caller(&request)?.clone();
        let request = request.into_inner();
        let backend = self.backend().await?;
        let id = blob_id(&request.blob_id)?;
        let now_ms = unix_millis(self.state.clock.now());
        let group_id = request.group_id;
        let device = caller.device_id.to_vec();
        let row = self
            .state
            .store
            .call(move |conn| {
                if !is_member(conn, &group_id, &device)? {
                    return Ok(Err(not_a_member()));
                }
                // A blob of another group is as good as none.
                Ok(match meta::find(conn, &id)? {
                    Some(row) if row.group_id == group_id && !row.expired(now_ms) => Ok(row),
                    _ => Err(Status::not_found("no such blob")),
                })
            })
            .await??;
        let ciphertext = backend
            .get(&id)
            .await
            .map_err(unavailable)?
            .ok_or_else(|| {
                tracing::error!("a blob's metadata has no bytes in the backend");
                Status::not_found("no such blob")
            })?;
        if Sha256::digest(&ciphertext).as_slice() != row.sha256.as_slice() {
            tracing::error!("a stored blob no longer matches its recorded hash");
            return Err(Status::data_loss("the stored blob is damaged"));
        }
        Ok(Response::new(pb::DownloadBlobResponse { ciphertext }))
    }

    // blob-store: Expiry and ack-delete.
    async fn ack_blob(
        &self,
        request: Request<pb::AckBlobRequest>,
    ) -> Result<Response<pb::AckBlobResponse>, Status> {
        let caller = caller(&request)?.clone();
        let request = request.into_inner();
        let backend = self.backend().await?;
        let id = blob_id(&request.blob_id)?;
        let now_ms = unix_millis(self.state.clock.now());
        let group_id = request.group_id;
        let device = caller.device_id.to_vec();
        let everyone_acked = self
            .state
            .store
            .call(move |conn| {
                let tx = conn.transaction()?;
                if !is_member(&tx, &group_id, &device)? {
                    return Ok(Err(not_a_member()));
                }
                // A blob that is gone, or not of this group, needs no ack.
                let Some(row) = meta::find(&tx, &id)?
                    .filter(|row| row.group_id == group_id && !row.expired(now_ms))
                else {
                    return Ok(Ok(false));
                };
                // The uploader counts as having acked its own blob.
                if device != row.uploader_device_id {
                    meta::add_ack(&tx, &id, &device)?;
                }
                let acks = meta::acks(&tx, &id)?;
                let done = GroupState::members(&tx, &group_id)?.iter().all(|member| {
                    let member = member.device_id.as_slice();
                    member == row.uploader_device_id.as_slice()
                        || acks.iter().any(|acked| acked.as_slice() == member)
                });
                tx.commit()?;
                Ok(Ok(done))
            })
            .await??;
        if everyone_acked {
            delete_blob(&self.state, &backend, &request.blob_id).await;
        }
        Ok(Response::new(pb::AckBlobResponse {}))
    }
}

/// Deletes a blob's bytes and then its row. A backend that cannot delete now
/// leaves both for the expiry sweep to retry.
async fn delete_blob(state: &State, backend: &Arc<dyn BlobStore>, blob_id_bytes: &[u8]) {
    let Ok(id) = BlobId::try_from(blob_id_bytes) else {
        return;
    };
    if let Err(error) = backend.delete(&id).await {
        tracing::warn!(%error, "could not delete a blob now; the sweep will retry");
        return;
    }
    let row_id = id.to_vec();
    if let Err(error) = state
        .store
        .call(move |conn| meta::delete(conn, &row_id))
        .await
    {
        tracing::warn!(%error, "could not forget a deleted blob");
    }
}

/// Deletes the blobs whose 7 days are up (blob-store: Expiry and ack-delete),
/// at the clock's current time. Run on its own every `sweep_interval`, and
/// by tests after moving the clock.
pub async fn sweep(state: &State) -> Result<(), StoreError> {
    let Some(backend) = state.blobs.clone() else {
        return Ok(());
    };
    let now_ms = unix_millis(state.clock.now());
    let expired = state
        .store
        .call(move |conn| meta::expired_ids(conn, now_ms))
        .await?;
    if !expired.is_empty() {
        tracing::debug!(count = expired.len(), "deleting expired blobs");
    }
    for id in expired {
        delete_blob(state, &backend, &id).await;
    }
    Ok(())
}

/// The expiry sweep as a background task, started with the server.
pub fn background(state: &State) -> Vec<JoinHandle<()>> {
    if state.blobs.is_none() {
        return Vec::new();
    }
    let sweeper = state.clone();
    vec![tokio::spawn(async move {
        let mut every = tokio::time::interval(sweeper.config.limits.sweep_interval);
        every.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            every.tick().await;
            if let Err(error) = sweep(&sweeper).await {
                tracing::error!(%error, "blob sweep failed");
            }
        }
    })]
}
