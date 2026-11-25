//! video-messages: what the server and the device core agree on for short video
//! messages (change `video-messages`). A video message is one MP4 clip (H.264
//! video + AAC audio) stored as an encrypted blob like an image (blob-store).
//!
//! Two rules are deliberate, owner-approved departures from the usual invariants
//! (owner, 2026-11-23), stated here and in `docs/architecture-decisions.md`:
//!
//! * **The size limit is config-driven, within a protocol ceiling.** The usual
//!   invariant is "limits live compile-time in `protocol/`". Here `protocol/`
//!   keeps a hard ceiling ([`MAX_VIDEO_BYTES`]) and a default
//!   ([`DEFAULT_MAX_VIDEO_BYTES`]); the server reads [`CONFIG_KEY`] from its
//!   config at startup and clamps it into `[the image cap, MAX_VIDEO_BYTES]`, so
//!   config can tune the cap without a rebuild but never past the safe bound.
//! * **Normalization runs in a native plugin, not the core.** The usual invariant
//!   is "the core owns every media transform". There is no viable pure-Rust mobile
//!   H.264 hardware transcode, so a platform plugin (AVFoundation / MediaCodec)
//!   transcodes gallery clips to the [`NORMALIZE`] target; the core does only the
//!   blob crypto. The constants of that target live here so both sides name one
//!   source of truth.

use crate::blob_store::{BLOB_NONCE_LEN, BLOB_TAG_LEN, MAX_BLOB_BYTES};

/// The longest a video message may run: 30 seconds (Config-driven limit). The
/// client enforces it on duration; the server never sees duration.
pub const MAX_VIDEO_DURATION_MS: u32 = 30_000;

/// The hard ceiling the server clamps the configured maximum blob size to
/// (Config-driven limit): 8 MiB. A 30-second clip at the normalize target is
/// about 4 MB, so this leaves headroom for a raised config without letting a bad
/// value set an unsafe cap. Stays within gRPC's default 4 MB only once; the blob
/// service raises its decode limit to match (server: Upload).
pub const MAX_VIDEO_BYTES: usize = 8 * 1024 * 1024;

/// The maximum blob size the server uses when its config sets none, or sets one
/// out of range (Config-driven limit): 5 MiB. Comfortably above a normalized
/// 30-second clip.
pub const DEFAULT_MAX_VIDEO_BYTES: usize = 5 * 1024 * 1024;

/// The config key, in the server's `[limits]` table, that tunes the maximum blob
/// size (Config-driven limit). Changing it is a config edit and a restart, no
/// rebuild.
pub const CONFIG_KEY: &str = "max_video_blob_bytes";

/// The canonical normalize target every video is brought to before it is
/// encrypted (locked, owner 2026-11-23): MP4 container, H.264 (AVC) video + AAC
/// audio, the long edge downscaled to [`NORMALIZE.long_edge`] px and the video
/// bitrate capped to about [`NORMALIZE.video_bitrate_bps`]. A recording is
/// captured straight to this; a gallery clip is transcoded to it by the native
/// plugin (Video normalization in a native plugin).
pub const NORMALIZE: NormalizeTarget = NormalizeTarget {
    long_edge: 854,
    video_bitrate_bps: 1_000_000,
};

/// The parameters of the [`NORMALIZE`] target, so the core, the plugin and the
/// docs name one source of truth. The container is MP4 and the codecs are H.264
/// video and AAC audio, which are fixed and so carried as constants, not fields.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NormalizeTarget {
    /// The longest edge in pixels after downscaling: 854 (480p long edge).
    pub long_edge: u32,
    /// The target H.264 bitrate in bits per second: about 1 Mbps.
    pub video_bitrate_bps: u32,
}

/// What every MP4 file starts with after its size prefix: the `ftyp` box type.
/// The core checks a clip's header names an `ftyp` box before handing it to a
/// player, the way audio checks the OGG capture pattern (Video messages).
pub const MP4_FTYP_MARKER: &[u8; 4] = b"ftyp";

/// The configured maximum blob size the server enforces and advertises, given
/// the operator's `max_video_blob_bytes` (Config-driven limit): the value is
/// clamped into `[MAX_BLOB_BYTES, MAX_VIDEO_BYTES]` (the image cap is the floor,
/// because the one cap also governs image blobs), and an absent value falls back
/// to [`DEFAULT_MAX_VIDEO_BYTES`]. A value above the ceiling is clamped down to
/// the ceiling, not rejected, so a generous config still starts the server.
pub fn clamp_max_blob_bytes(configured: Option<u64>) -> u64 {
    let floor = MAX_BLOB_BYTES as u64;
    let ceiling = MAX_VIDEO_BYTES as u64;
    match configured {
        None => DEFAULT_MAX_VIDEO_BYTES as u64,
        Some(value) => value.clamp(floor, ceiling),
    }
}

/// The largest `UploadBlob` request the server decodes, given the configured
/// maximum blob size (Config-driven limit; server: Upload): the blob plus 1 KiB
/// for the ids around it, the video analogue of
/// [`crate::blob_store::MAX_UPLOAD_REQUEST_BYTES`]. Sized from the configured cap
/// so a legitimately video-sized blob is not refused before it is buffered.
pub fn max_upload_request_bytes(max_blob_bytes: u64) -> usize {
    usize::try_from(max_blob_bytes)
        .unwrap_or(MAX_VIDEO_BYTES)
        .saturating_add(1024)
}

/// The ciphertext size of a blob holding `plaintext_len` plaintext bytes: the
/// nonce, the plaintext and the tag (blob-store: Blob crypto and send order). The
/// core fills `Video.blob_size` with this.
pub fn blob_size(plaintext_len: usize) -> u64 {
    (plaintext_len + BLOB_NONCE_LEN + BLOB_TAG_LEN) as u64
}
