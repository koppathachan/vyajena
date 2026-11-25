//! blob-store on the server (change `media-blobs`): upload, download, expiry and
//! ack-delete, opaque ciphertext, the pluggable backend and graceful
//! degradation.
//!
//! Cast (support::messaging::World): the admin (`akhil`), Alice's laptop and
//! Bob's phone, all in the main group. Alice's laptop uploads unless a test
//! says otherwise.

mod support;

use std::collections::HashMap;
use std::os::unix::fs::PermissionsExt;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use protocol::blob_store::{
    BLOB_TTL, MAX_BLOB_BYTES, MAX_BLOB_STORE_BYTES, MAX_UPLOADS_PER_DEVICE_PER_HOUR,
};
use server::blobs::{BackendError, BlobId, BlobStore};
use support::clients::TestDevice;
use support::harness::{Blobs, TestServer, authed};
use support::messaging::{GROUP_ID, World, envelope, world_on};
use support::pb;
use support::pb::envelope::Kind;
use support::{Check, invite_user};
use tonic::Code;

fn blob_id(n: u8) -> Vec<u8> {
    vec![n; 16]
}

/// Bytes that stand in for ciphertext: the server never reads them.
fn ciphertext(len: usize) -> Vec<u8> {
    (0..len).map(|i| (i % 251) as u8).collect()
}

async fn filesystem() -> World {
    world_on(TestServer::start_with_blobs(Blobs::Filesystem).await).await
}

/// A filesystem-backed world whose one blob cap is pinned to `cap` bytes
/// (video-messages: Config-driven limit), so a test can exercise the configured
/// upload cap without allocating multi-megabyte blobs.
async fn filesystem_capped(cap: u64) -> World {
    let config = format!("[limits]\n{} = {cap}\n", protocol::video::CONFIG_KEY);
    world_on(TestServer::start_with_blobs_and_config(Blobs::Filesystem, &config).await).await
}

/// Session tokens last six hours: after the clock moves further, the devices
/// sign in again.
async fn sign_in_again(w: &mut World) {
    for device in [&mut w.admin, &mut w.laptop, &mut w.phone] {
        device.authenticate(&w.conn).await;
    }
}

async fn upload(
    w: &World,
    who: &TestDevice,
    group: &[u8],
    id: &[u8],
    bytes: &[u8],
) -> Result<pb::UploadBlobResponse, tonic::Status> {
    w.conn
        .blobs()
        .upload_blob(authed(
            who.token(),
            pb::UploadBlobRequest {
                group_id: group.to_vec(),
                blob_id: id.to_vec(),
                ciphertext: bytes.to_vec(),
            },
        ))
        .await
        .map(|response| response.into_inner())
}

async fn download(
    w: &World,
    who: &TestDevice,
    group: &[u8],
    id: &[u8],
) -> Result<Vec<u8>, tonic::Status> {
    w.conn
        .blobs()
        .download_blob(authed(
            who.token(),
            pb::DownloadBlobRequest {
                group_id: group.to_vec(),
                blob_id: id.to_vec(),
            },
        ))
        .await
        .map(|response| response.into_inner().ciphertext)
}

async fn ack(
    w: &World,
    who: &TestDevice,
    group: &[u8],
    id: &[u8],
) -> Result<pb::AckBlobResponse, tonic::Status> {
    w.conn
        .blobs()
        .ack_blob(authed(
            who.token(),
            pb::AckBlobRequest {
                group_id: group.to_vec(),
                blob_id: id.to_vec(),
            },
        ))
        .await
        .map(|response| response.into_inner())
}

/// The names of the files in the blob directory (probe and temp files aside).
fn stored(server: &TestServer) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(server.blob_dir())
        .check("blob directory")
        .map(|entry| {
            entry
                .check("entry")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .filter(|name| !name.starts_with('.'))
        .collect();
    names.sort();
    names
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn seconds(timestamp: Option<prost_types::Timestamp>) -> i64 {
    timestamp.check("expires_at").seconds
}

// spec: blob-store/upload
#[tokio::test]
async fn a_member_uploads_a_blob_and_gets_an_expiry_seven_days_out() {
    let w = filesystem().await;
    let now = w.server.clock_now();
    let response = upload(&w, &w.laptop, GROUP_ID, &blob_id(1), &ciphertext(1_500_000))
        .await
        .check("UploadBlob");
    let uploaded = now
        .duration_since(std::time::UNIX_EPOCH)
        .check("time")
        .as_secs();
    assert_eq!(
        seconds(response.expires_at),
        i64::try_from(uploaded + BLOB_TTL.as_secs()).check("seconds")
    );
    assert_eq!(stored(&w.server), vec![hex(&blob_id(1))]);
}

// spec: blob-store/upload
#[tokio::test]
async fn the_largest_blob_is_accepted_and_one_byte_more_is_not() {
    // The one blob cap is config-driven (video-messages: Config-driven limit);
    // pin it to the image cap so the boundary is small and exact.
    let cap = MAX_BLOB_BYTES;
    let w = filesystem_capped(cap as u64).await;
    upload(&w, &w.laptop, GROUP_ID, &blob_id(1), &ciphertext(cap))
        .await
        .check("a blob of the configured cap");
    let over = upload(&w, &w.laptop, GROUP_ID, &blob_id(2), &ciphertext(cap + 1)).await;
    assert_eq!(over.err().map(|s| s.code()), Some(Code::InvalidArgument));
    // Nothing was stored for the refused one.
    assert_eq!(stored(&w.server), vec![hex(&blob_id(1))]);
}

// spec: video-messages/config-driven-limit
#[tokio::test]
async fn the_default_cap_accepts_a_video_sized_blob_above_the_image_cap() {
    // With no config the cap is the video default (5 MiB), above the image cap
    // (~2 MiB), so a blob that the old image-sized cap would have refused is now
    // accepted — a video blob is a blob.
    let w = filesystem().await;
    assert_eq!(
        w.server.state.config.limits.max_video_blob_bytes,
        protocol::video::DEFAULT_MAX_VIDEO_BYTES as u64
    );
    upload(
        &w,
        &w.laptop,
        GROUP_ID,
        &blob_id(1),
        &ciphertext(MAX_BLOB_BYTES + 100_000),
    )
    .await
    .check("a blob above the image cap, under the video default");
    assert_eq!(stored(&w.server), vec![hex(&blob_id(1))]);
}

// spec: video-messages/config-driven-limit
#[tokio::test]
async fn raising_the_config_raises_the_enforced_upload_cap() {
    // The operator raises the cap; the server enforces the new value with no
    // rebuild. A blob at the configured cap is accepted, one byte more refused.
    let cap = MAX_BLOB_BYTES as u64 + 500_000;
    let w = filesystem_capped(cap).await;
    assert_eq!(w.server.state.config.limits.max_video_blob_bytes, cap);
    upload(
        &w,
        &w.laptop,
        GROUP_ID,
        &blob_id(1),
        &ciphertext(cap as usize),
    )
    .await
    .check("a blob at the raised cap");
    let over = upload(
        &w,
        &w.laptop,
        GROUP_ID,
        &blob_id(2),
        &ciphertext(cap as usize + 1),
    )
    .await;
    assert_eq!(over.err().map(|s| s.code()), Some(Code::InvalidArgument));
    assert_eq!(stored(&w.server), vec![hex(&blob_id(1))]);
}

// spec: blob-store/upload
#[tokio::test]
async fn an_empty_blob_or_a_short_id_is_invalid() {
    let w = filesystem().await;
    let empty = upload(&w, &w.laptop, GROUP_ID, &blob_id(1), &[]).await;
    assert_eq!(empty.err().map(|s| s.code()), Some(Code::InvalidArgument));
    let short = upload(&w, &w.laptop, GROUP_ID, &[1, 2, 3], &ciphertext(10)).await;
    assert_eq!(short.err().map(|s| s.code()), Some(Code::InvalidArgument));
    assert!(stored(&w.server).is_empty());
}

// spec: blob-store/upload
#[tokio::test]
async fn a_device_outside_the_group_cannot_upload() {
    let w = filesystem().await;
    // Carol has registered but was never added to the group.
    let (_carol, carol_device) = invite_user(&w.conn, &w.admin, "carol").await;
    let refused = upload(&w, &carol_device, GROUP_ID, &blob_id(1), &ciphertext(100)).await;
    assert_eq!(
        refused.err().map(|s| s.code()),
        Some(Code::PermissionDenied)
    );
    assert!(stored(&w.server).is_empty());
}

// spec: blob-store/upload
#[tokio::test]
async fn uploading_the_same_bytes_again_is_idempotent() {
    let w = filesystem().await;
    let bytes = ciphertext(5_000);
    let first = upload(&w, &w.laptop, GROUP_ID, &blob_id(1), &bytes)
        .await
        .check("first upload");
    // Time passes before the retry: it still gets the original expiry.
    w.server.clock.advance(Duration::from_secs(600));
    let again = upload(&w, &w.laptop, GROUP_ID, &blob_id(1), &bytes)
        .await
        .check("retry");
    assert_eq!(first.expires_at, again.expires_at);
    assert_eq!(stored(&w.server).len(), 1);
}

// spec: blob-store/upload
#[tokio::test]
async fn other_bytes_under_a_live_blob_id_are_refused() {
    let w = filesystem().await;
    let bytes = ciphertext(5_000);
    upload(&w, &w.laptop, GROUP_ID, &blob_id(1), &bytes)
        .await
        .check("upload");
    let clash = upload(&w, &w.phone, GROUP_ID, &blob_id(1), &ciphertext(5_001)).await;
    assert_eq!(clash.err().map(|s| s.code()), Some(Code::AlreadyExists));
    // The first blob is untouched.
    assert_eq!(
        download(&w, &w.phone, GROUP_ID, &blob_id(1))
            .await
            .check("download"),
        bytes
    );
}

// spec: blob-store/download
#[tokio::test]
async fn a_member_downloads_exactly_what_was_uploaded() {
    let w = filesystem().await;
    let bytes = ciphertext(300_000);
    upload(&w, &w.laptop, GROUP_ID, &blob_id(1), &bytes)
        .await
        .check("upload");
    for device in [&w.phone, &w.admin, &w.laptop] {
        assert_eq!(
            download(&w, device, GROUP_ID, &blob_id(1))
                .await
                .check("download"),
            bytes
        );
    }
}

// spec: blob-store/download
#[tokio::test]
async fn a_device_outside_the_group_cannot_download() {
    let w = filesystem().await;
    upload(&w, &w.laptop, GROUP_ID, &blob_id(1), &ciphertext(100))
        .await
        .check("upload");
    let (_carol, carol_device) = invite_user(&w.conn, &w.admin, "carol").await;
    let refused = download(&w, &carol_device, GROUP_ID, &blob_id(1)).await;
    assert_eq!(
        refused.err().map(|s| s.code()),
        Some(Code::PermissionDenied)
    );
    // Claiming a group it is not in does not help either.
    let unknown = download(&w, &w.phone, b"another-group", &blob_id(1)).await;
    assert_eq!(
        unknown.err().map(|s| s.code()),
        Some(Code::PermissionDenied)
    );
}

// spec: blob-store/download
#[tokio::test]
async fn an_unknown_blob_is_not_found() {
    let w = filesystem().await;
    let missing = download(&w, &w.phone, GROUP_ID, &blob_id(9)).await;
    assert_eq!(missing.err().map(|s| s.code()), Some(Code::NotFound));
}

// spec: blob-store/download
#[tokio::test]
async fn an_expired_blob_is_not_found_even_before_the_sweep() {
    let mut w = filesystem().await;
    upload(&w, &w.laptop, GROUP_ID, &blob_id(1), &ciphertext(100))
        .await
        .check("upload");
    w.server.clock.advance(BLOB_TTL - Duration::from_secs(1));
    sign_in_again(&mut w).await;
    assert!(download(&w, &w.phone, GROUP_ID, &blob_id(1)).await.is_ok());
    w.server.clock.advance(Duration::from_secs(1));
    sign_in_again(&mut w).await;
    let gone = download(&w, &w.phone, GROUP_ID, &blob_id(1)).await;
    assert_eq!(gone.err().map(|s| s.code()), Some(Code::NotFound));
}

// spec: blob-store/expiry-and-ack-delete
#[tokio::test]
async fn the_blob_is_deleted_once_every_member_device_acked() {
    let w = filesystem().await;
    upload(&w, &w.laptop, GROUP_ID, &blob_id(1), &ciphertext(100))
        .await
        .check("upload");
    // The uploader counts as acked; the admin and Bob's phone still need it.
    ack(&w, &w.admin, GROUP_ID, &blob_id(1)).await.check("ack");
    assert_eq!(stored(&w.server).len(), 1, "one device has not acked yet");
    // An ack is idempotent.
    ack(&w, &w.admin, GROUP_ID, &blob_id(1))
        .await
        .check("ack again");
    assert_eq!(stored(&w.server).len(), 1);
    ack(&w, &w.phone, GROUP_ID, &blob_id(1)).await.check("ack");
    assert!(stored(&w.server).is_empty(), "every device acked");
    let gone = download(&w, &w.phone, GROUP_ID, &blob_id(1)).await;
    assert_eq!(gone.err().map(|s| s.code()), Some(Code::NotFound));
    // Acking what is gone still succeeds, so a retry is safe.
    ack(&w, &w.phone, GROUP_ID, &blob_id(1))
        .await
        .check("late ack");
}

// spec: blob-store/expiry-and-ack-delete
#[tokio::test]
async fn only_a_member_may_ack() {
    let w = filesystem().await;
    upload(&w, &w.laptop, GROUP_ID, &blob_id(1), &ciphertext(100))
        .await
        .check("upload");
    let (_carol, carol_device) = invite_user(&w.conn, &w.admin, "carol").await;
    let refused = ack(&w, &carol_device, GROUP_ID, &blob_id(1)).await;
    assert_eq!(
        refused.err().map(|s| s.code()),
        Some(Code::PermissionDenied)
    );
    assert_eq!(stored(&w.server).len(), 1);
}

// spec: blob-store/expiry-and-ack-delete
#[tokio::test]
async fn a_blob_nobody_acks_is_deleted_after_seven_days() {
    let w = filesystem().await;
    upload(&w, &w.laptop, GROUP_ID, &blob_id(1), &ciphertext(100))
        .await
        .check("upload");
    w.server.clock.advance(BLOB_TTL - Duration::from_secs(1));
    w.server.sweep().await;
    assert_eq!(stored(&w.server).len(), 1, "not yet");
    w.server.clock.advance(Duration::from_secs(1));
    w.server.sweep().await;
    assert!(stored(&w.server).is_empty(), "the 7 days are up");
    let rows: i64 = w
        .server
        .query(|db| db.query_row("SELECT count(*) FROM blobs", [], |r| r.get(0)))
        .await;
    assert_eq!(rows, 0, "the metadata is gone too");
}

// spec: blob-store/expiry-and-ack-delete
#[tokio::test]
async fn an_expired_blob_id_can_be_used_again() {
    let mut w = filesystem().await;
    upload(&w, &w.laptop, GROUP_ID, &blob_id(1), &ciphertext(100))
        .await
        .check("upload");
    w.server.clock.advance(BLOB_TTL);
    sign_in_again(&mut w).await;
    // The sweep has not run: the old row is expired, so a new upload replaces it.
    upload(&w, &w.phone, GROUP_ID, &blob_id(1), &ciphertext(200))
        .await
        .check("reuse the id");
    assert_eq!(
        download(&w, &w.laptop, GROUP_ID, &blob_id(1))
            .await
            .check("download"),
        ciphertext(200)
    );
}

// spec: blob-store/blobs-are-opaque-ciphertext
#[tokio::test]
async fn the_server_stores_the_bytes_unchanged_and_privately() {
    let w = filesystem().await;
    let bytes = ciphertext(40_000);
    upload(&w, &w.laptop, GROUP_ID, &blob_id(1), &bytes)
        .await
        .check("upload");
    let path = w.server.blob_dir().join(hex(&blob_id(1)));
    assert_eq!(std::fs::read(&path).check("blob file"), bytes);
    let mode = std::fs::metadata(&path)
        .check("metadata")
        .permissions()
        .mode();
    assert_eq!(mode & 0o777, 0o600, "owner-only");
    // The database holds a hash and sizes, never the bytes.
    let (size, hash_len): (i64, i64) = w
        .server
        .query(|db| {
            db.query_row("SELECT size, length(sha256) FROM blobs", [], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
        })
        .await;
    assert_eq!((size, hash_len), (40_000, 32));
}

/// A backend that keeps blobs in memory: the server's rules are the same on it.
#[derive(Default)]
struct InMemory {
    blobs: Mutex<HashMap<BlobId, Vec<u8>>>,
}

impl InMemory {
    fn all(&self) -> HashMap<BlobId, Vec<u8>> {
        self.blobs
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }
}

#[tonic::async_trait]
impl BlobStore for InMemory {
    async fn check(&self) -> Result<(), BackendError> {
        Ok(())
    }
    async fn put(&self, id: &BlobId, bytes: Vec<u8>) -> Result<(), BackendError> {
        self.blobs
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(*id, bytes);
        Ok(())
    }
    async fn get(&self, id: &BlobId) -> Result<Option<Vec<u8>>, BackendError> {
        Ok(self.all().get(id).cloned())
    }
    async fn delete(&self, id: &BlobId) -> Result<(), BackendError> {
        self.blobs
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(id);
        Ok(())
    }
    async fn exists(&self, id: &BlobId) -> Result<bool, BackendError> {
        Ok(self.all().contains_key(id))
    }
}

// spec: blob-store/pluggable-storage-backend
#[tokio::test]
async fn another_backend_changes_nothing_the_server_decides() {
    let backend = Arc::new(InMemory::default());
    let w = world_on(TestServer::start_with_blobs(Blobs::Backend(backend.clone())).await).await;
    let bytes = ciphertext(2_000);

    upload(&w, &w.laptop, GROUP_ID, &blob_id(1), &bytes)
        .await
        .check("upload");
    // The backend holds opaque bytes by id and nothing else.
    let id: BlobId = [1; 16];
    assert_eq!(backend.all(), HashMap::from([(id, bytes.clone())]));

    // Membership is still the server's decision, from its own database.
    let (_carol, carol_device) = invite_user(&w.conn, &w.admin, "carol").await;
    let refused = download(&w, &carol_device, GROUP_ID, &blob_id(1)).await;
    assert_eq!(
        refused.err().map(|s| s.code()),
        Some(Code::PermissionDenied)
    );
    assert_eq!(
        download(&w, &w.phone, GROUP_ID, &blob_id(1))
            .await
            .check("download"),
        bytes
    );

    // So are acks and expiry.
    ack(&w, &w.admin, GROUP_ID, &blob_id(1)).await.check("ack");
    ack(&w, &w.phone, GROUP_ID, &blob_id(1)).await.check("ack");
    assert!(backend.all().is_empty(), "ack-delete reached the backend");

    upload(&w, &w.laptop, GROUP_ID, &blob_id(2), &bytes)
        .await
        .check("upload");
    w.server.clock.advance(BLOB_TTL);
    w.server.sweep().await;
    assert!(backend.all().is_empty(), "expiry reached the backend");
}

/// All three RPCs answer `UNAVAILABLE`.
async fn assert_all_unavailable(w: &World) {
    let up = upload(w, &w.laptop, GROUP_ID, &blob_id(1), &ciphertext(10)).await;
    let down = download(w, &w.phone, GROUP_ID, &blob_id(1)).await;
    let acked = ack(w, &w.phone, GROUP_ID, &blob_id(1)).await;
    assert_eq!(up.err().map(|s| s.code()), Some(Code::Unavailable));
    assert_eq!(down.err().map(|s| s.code()), Some(Code::Unavailable));
    assert_eq!(acked.err().map(|s| s.code()), Some(Code::Unavailable));
}

/// Messaging still works: a message is accepted and the other device gets it.
async fn assert_messaging_works(w: &mut World) {
    let epoch = w.laptop.epoch();
    let text = w.laptop.message(b"still talking");
    let seq = w
        .laptop_stream
        .accepted(envelope(Kind::Application, epoch, text))
        .await;
    let got = w.phone_stream.envelope().await;
    assert_eq!(got.server_seq, seq);
    assert_eq!(w.phone.read_message(&got.ciphertext), b"still talking");
}

// spec: blob-store/blob-storage-and-graceful-degradation
// spec: server-runtime/blob-storage-degrades-gracefully
#[tokio::test]
async fn with_no_blob_store_configured_images_are_off_and_text_works() {
    let mut w = world_on(TestServer::start_with_blobs(Blobs::Off).await).await;
    assert_all_unavailable(&w).await;
    assert_messaging_works(&mut w).await;
}

// spec: blob-store/blob-storage-and-graceful-degradation
// spec: server-runtime/blob-storage-degrades-gracefully
#[tokio::test]
async fn an_unmounted_drive_is_unavailable_until_it_is_back_and_text_works() {
    let mut w = world_on(TestServer::start_with_blobs(Blobs::MissingDirectory).await).await;
    assert_all_unavailable(&w).await;
    assert_messaging_works(&mut w).await;
    assert!(
        !w.server.blob_dir().exists(),
        "the server never creates the directory"
    );

    // The drive is mounted at the stable path and the server restarted.
    w.server.mount_blob_dir();
    w.server.restart().await;
    let conn = w.server.connect().await;
    w.laptop.authenticate(&conn).await;
    w.phone.authenticate(&conn).await;
    w.conn = conn;
    upload(&w, &w.laptop, GROUP_ID, &blob_id(1), &ciphertext(10))
        .await
        .check("upload after the drive is back");
    assert_eq!(
        download(&w, &w.phone, GROUP_ID, &blob_id(1))
            .await
            .check("download"),
        ciphertext(10)
    );
}

// spec: blob-store/blob-storage-and-graceful-degradation
#[tokio::test]
async fn a_drive_that_is_pulled_later_stops_the_blobs_and_never_the_text() {
    let mut w = filesystem().await;
    upload(&w, &w.laptop, GROUP_ID, &blob_id(1), &ciphertext(10))
        .await
        .check("upload");
    std::fs::remove_dir_all(w.server.blob_dir()).check("pull the drive");
    assert_all_unavailable(&w).await;
    assert!(
        !w.server.blob_dir().exists(),
        "nothing was written to the main storage"
    );
    assert_messaging_works(&mut w).await;
}

// --- upload-limits (change `upload-limits`) ------------------------------------

/// Records a blob row for `device` without any bytes behind it: the limits read
/// only the metadata.
async fn seed_row(w: &World, device: &[u8], n: u32, size: i64, age_ms: i64) {
    let now_ms = i64::try_from(
        w.server
            .state
            .clock
            .now()
            .duration_since(std::time::UNIX_EPOCH)
            .check("time")
            .as_millis(),
    )
    .check("ms");
    let device = device.to_vec();
    let mut id = vec![0u8; 16];
    id[..4].copy_from_slice(&n.to_be_bytes());
    w.server
        .state
        .store
        .call(move |conn| {
            Ok(conn.execute(
                "INSERT INTO blobs (blob_id, group_id, uploader_device_id, size, sha256, uploaded_at, expires_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                rusqlite::params![
                    id,
                    GROUP_ID,
                    device,
                    size,
                    vec![7u8; 32],
                    now_ms - age_ms,
                    now_ms - age_ms + 7 * 24 * 3_600_000
                ],
            )?)
        })
        .await
        .check("seed a blob row");
}

// spec: blob-store/upload-limits
#[tokio::test]
async fn a_full_store_refuses_new_uploads_and_a_retry_still_works() {
    let w = filesystem().await;
    upload(&w, &w.laptop, GROUP_ID, &blob_id(1), &ciphertext(1000))
        .await
        .check("first upload");
    // The store is one byte short of full for a 1000-byte blob.
    let room = i64::try_from(MAX_BLOB_STORE_BYTES).check("cap") - 1000 - 999;
    seed_row(&w, &w.phone.device_id, 1, room, 0).await;
    let refused = upload(&w, &w.laptop, GROUP_ID, &blob_id(2), &ciphertext(1000)).await;
    assert_eq!(
        refused.err().map(|s| s.code()),
        Some(Code::ResourceExhausted)
    );
    assert_eq!(stored(&w.server), vec![hex(&blob_id(1))], "nothing stored");
    // An identical re-upload is not a new blob: same answer, no limit.
    upload(&w, &w.laptop, GROUP_ID, &blob_id(1), &ciphertext(1000))
        .await
        .check("idempotent retry");
    // A smaller blob that fits is accepted.
    upload(&w, &w.laptop, GROUP_ID, &blob_id(3), &ciphertext(900))
        .await
        .check("a blob that fits");
}

// spec: blob-store/upload-limits
#[tokio::test]
async fn expired_blobs_free_room_in_the_store() {
    let w = filesystem().await;
    // Full, but the blob that fills it is 8 days old: expired.
    seed_row(
        &w,
        &w.phone.device_id,
        1,
        i64::try_from(MAX_BLOB_STORE_BYTES).check("cap"),
        8 * 24 * 3_600_000,
    )
    .await;
    upload(&w, &w.laptop, GROUP_ID, &blob_id(2), &ciphertext(1000))
        .await
        .check("expired rows do not count");
}

// spec: blob-store/upload-limits
#[tokio::test]
async fn one_device_is_limited_per_hour_and_others_are_not() {
    let w = filesystem().await;
    for n in 0..MAX_UPLOADS_PER_DEVICE_PER_HOUR {
        seed_row(&w, &w.laptop.device_id, n + 1, 10, 60_000).await;
    }
    let limited = upload(&w, &w.laptop, GROUP_ID, &blob_id(1), &ciphertext(100)).await;
    assert_eq!(
        limited.err().map(|s| s.code()),
        Some(Code::ResourceExhausted)
    );
    // Bob's phone is not limited by Alice's count.
    upload(&w, &w.phone, GROUP_ID, &blob_id(2), &ciphertext(100))
        .await
        .check("another device");
}

// spec: blob-store/upload-limits
#[tokio::test]
async fn uploads_older_than_an_hour_do_not_count() {
    let w = filesystem().await;
    for n in 0..MAX_UPLOADS_PER_DEVICE_PER_HOUR {
        seed_row(&w, &w.laptop.device_id, n + 1, 10, 2 * 3_600_000).await;
    }
    upload(&w, &w.laptop, GROUP_ID, &blob_id(1), &ciphertext(100))
        .await
        .check("old uploads do not count");
}

// spec: blob-store/upload-limits
#[tokio::test]
async fn a_request_far_over_the_cap_is_refused_before_the_handler() {
    let w = filesystem().await;
    // Far over the configured blob cap (video-messages: Config-driven limit), so
    // the decode limit (the cap plus a little) refuses it before the handler.
    let far_over = w.server.state.config.limits.max_video_blob_bytes as usize + 1_000_000;
    let huge = upload(&w, &w.laptop, GROUP_ID, &blob_id(1), &ciphertext(far_over)).await;
    let code = huge.err().map(|s| s.code());
    assert!(
        matches!(code, Some(Code::OutOfRange | Code::ResourceExhausted)),
        "{code:?}"
    );
    assert!(stored(&w.server).is_empty(), "nothing was stored");
}
