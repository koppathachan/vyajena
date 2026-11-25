//! photo-sticker: what the device core and the native segmentation plugins
//! agree on for an on-device subject cutout (change `photo-sticker`).
//!
//! A sticker is sent as an ordinary transparent `Image` (blob-store,
//! message-content: Image messages), so there is **no new wire constant** here:
//! the cutout rides the existing image body and reuses
//! [`crate::blob_store`]'s `MAX_IMAGE_EDGE`, `MAX_IMAGE_BYTES`, `WEBP_QUALITY`,
//! `MAX_THUMBHASH_BYTES` and `BLOB_*` unchanged. This module only fixes the
//! **seam** between the native producer and the core: the RGBA contract every
//! [`SegmentationSource`] fills, and the names of the sources.

/// Bytes per pixel of the RGBA cutout a [`SegmentationSource`] returns: 8-bit
/// R, G, B, A in that byte order (Segmentation seam and RGBA contract).
pub const RGBA_BYTES_PER_PIXEL: usize = 4;

/// The exact length, in bytes, of the RGBA buffer for a `width`×`height` cutout:
/// the row stride is `width` × [`RGBA_BYTES_PER_PIXEL`] and there are `height`
/// rows, with no padding (Segmentation seam and RGBA contract). The core refuses
/// a buffer that is not this long.
pub const fn rgba_buffer_len(width: u32, height: u32) -> usize {
    (width as usize) * (height as usize) * RGBA_BYTES_PER_PIXEL
}

/// Who produced a subject cutout (Segmentation seam and RGBA contract).
///
/// Every cutout crosses the seam as the same straight-alpha, 8-bit-per-channel
/// RGBA image in the sRGB color space (R,G,B,A byte order, row stride
/// `width`×4, orientation already applied) whatever its source. A new engine is
/// added by registering a variant here and meeting the contract; the encode step
/// and the send path do not change.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum SegmentationSource {
    /// iOS Vision foreground/subject mask (MVP).
    IosVision,
    /// Android ML Kit Subject Segmentation (MVP).
    AndroidMlKit,
    /// A future consented server path; reserved, not registered in the MVP.
    Server,
    /// A future on-device ONNX/BiRefNet model; reserved, not registered in the
    /// MVP.
    OnnxLocal,
}

impl SegmentationSource {
    /// Whether this source is one the MVP registers and runs on device (iOS
    /// Vision, Android ML Kit). The reserved [`Self::Server`] and
    /// [`Self::OnnxLocal`] are future work and return `false`.
    pub const fn is_mvp(self) -> bool {
        matches!(self, Self::IosVision | Self::AndroidMlKit)
    }

    /// Whether this source runs entirely on the device without uploading the
    /// photo or downloading a model. True for every MVP source; the reserved
    /// [`Self::Server`] does not.
    pub const fn is_on_device(self) -> bool {
        !matches!(self, Self::Server)
    }
}
