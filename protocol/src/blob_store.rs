//! blob-store, message-content (image messages): what the server and the device
//! core agree on for encrypted image blobs (change `media-blobs`).

use std::time::Duration;

use crate::DAY;

/// Length of a blob id, chosen at random by the sender (Image messages).
pub const BLOB_ID_LEN: usize = 16;

/// Length of a blob's XChaCha20-Poly1305 key, fresh per blob (Image messages).
pub const BLOB_KEY_LEN: usize = 32;

/// Length of the XChaCha20-Poly1305 nonce stored before the ciphertext.
pub const BLOB_NONCE_LEN: usize = 24;

/// Length of the Poly1305 tag stored after the ciphertext.
pub const BLOB_TAG_LEN: usize = 16;

/// The label the nonce of a blob is derived under (Blob crypto and send
/// order): `nonce = HMAC-SHA256(blob_key, BLOB_NONCE_CONTEXT || blob_id)`,
/// first [`BLOB_NONCE_LEN`] bytes. A key encrypts one plaintext only, so the
/// derived nonce is never reused with different data, and a retried upload is
/// byte for byte the same.
pub const BLOB_NONCE_CONTEXT: &[u8] = b"vyajena2-blob-nonce-v1";

/// The most bytes of encoded WebP a sent image may hold: 2 MB (Image encode
/// pipeline).
pub const MAX_IMAGE_BYTES: usize = 2 * 1024 * 1024;

/// The longest edge, in pixels, of a sent image (Image encode pipeline).
pub const MAX_IMAGE_EDGE: u32 = 2048;

/// The WebP quality the core starts from (Image encode pipeline).
pub const WEBP_QUALITY: f32 = 80.0;

/// The most bytes of ThumbHash an `Image` carries (Image messages). A ThumbHash
/// is about 25 bytes; this leaves room without letting a sender stuff the
/// message.
pub const MAX_THUMBHASH_BYTES: usize = 64;

/// The most bytes of ciphertext a blob holds: the largest image plus the nonce
/// and the tag (Upload). Within gRPC's default 4 MB message size.
pub const MAX_BLOB_BYTES: usize = MAX_IMAGE_BYTES + BLOB_NONCE_LEN + BLOB_TAG_LEN;

/// The most bytes of live blobs the server keeps in all; an upload that would go
/// over is refused with `RESOURCE_EXHAUSTED` (Upload limits; change
/// `upload-limits` D1).
pub const MAX_BLOB_STORE_BYTES: u64 = 1024 * 1024 * 1024;

/// The most new blobs one device may upload in an hour (Upload limits).
pub const MAX_UPLOADS_PER_DEVICE_PER_HOUR: u32 = 60;

/// The most uploads the server handles at once; each is buffered whole in memory
/// (Upload limits).
pub const MAX_CONCURRENT_UPLOADS: usize = 8;

/// The largest `UploadBlob` request the server decodes: the blob plus 1 KiB for
/// the ids around it (Upload limits).
pub const MAX_UPLOAD_REQUEST_BYTES: usize = MAX_BLOB_BYTES + 1024;

/// A blob is deleted this long after its upload, whatever its acks (Expiry and
/// ack-delete).
pub const BLOB_TTL: Duration = Duration::from_secs(7 * DAY);
