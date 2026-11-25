//! video-messages: the core's crypto-only role for video messages (change
//! `video-messages`). A video is normalized to canonical H.264/MP4 by a native
//! platform plugin (AVFoundation / MediaCodec), **not** here — there is no viable
//! pure-Rust mobile H.264 transcode, so this is a documented departure from "the
//! core owns every media transform" (client-runtime: Video normalization in a
//! native plugin). The core still owns the blob crypto and the first-frame
//! ThumbHash: the plugin hands over the normalized MP4 bytes and the first frame
//! as RGBA, and this module turns that frame into the ThumbHash the `Video` body
//! carries, so the blur is computed the same way as an image's.

use image::{DynamicImage, RgbaImage, imageops::FilterType};
use protocol::blob_store::MAX_THUMBHASH_BYTES;
use protocol::video::MP4_FTYP_MARKER;

/// The longest edge a ThumbHash input may have (the crate's limit), as in the
/// image pipeline.
const THUMBHASH_EDGE: u32 = 100;

/// Whether `bytes` look like an MP4 file: a leading box whose type is `ftyp`
/// (video-messages: Video messages). The core checks this before caching a clip
/// or handing a download to a player, as audio checks the OGG capture pattern. It
/// is a cheap shape check, not a full demux.
pub fn looks_like_mp4(bytes: &[u8]) -> bool {
    // An MP4 begins with a 4-byte big-endian box size, then the 4-byte box type.
    bytes.len() > 8 && &bytes[4..8] == MP4_FTYP_MARKER
}

/// The ThumbHash of a first frame given as RGBA pixels (`width` x `height`), for
/// the `Video` body's blur (video-messages: Video messages). `None` if the buffer
/// is empty or does not match the dimensions, in which case the body carries no
/// ThumbHash and the bubble shows a neutral placeholder. Computed in the core so
/// the ThumbHash math stays with the image pipeline's.
pub fn first_frame_thumbhash(rgba: &[u8], width: u32, height: u32) -> Option<Vec<u8>> {
    if width == 0 || height == 0 {
        return None;
    }
    let expected = (width as usize)
        .checked_mul(height as usize)?
        .checked_mul(4)?;
    if rgba.len() != expected {
        return None;
    }
    let image = DynamicImage::ImageRgba8(RgbaImage::from_raw(width, height, rgba.to_vec())?);
    let small = scale_to(&image, THUMBHASH_EDGE).to_rgba8();
    let hash = thumbhash::rgba_to_thumb_hash(
        small.width() as usize,
        small.height() as usize,
        small.as_raw(),
    );
    debug_assert!(hash.len() <= MAX_THUMBHASH_BYTES);
    Some(hash)
}

/// `image` scaled so its longest edge is at most `edge`, keeping aspect ratio; an
/// already-small image is returned unchanged.
fn scale_to(image: &DynamicImage, edge: u32) -> DynamicImage {
    let (width, height) = (image.width(), image.height());
    if width.max(height) <= edge {
        return image.clone();
    }
    image.resize(edge, edge, FilterType::Triangle)
}
