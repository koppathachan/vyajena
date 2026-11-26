//! client-runtime: Image encode pipeline. Every image transform lives here, in
//! the core: Dart hands over the picked bytes and nothing else touches pixels
//! for the wire (design D5).
//!
//! Decode, apply the EXIF orientation, drop all metadata by encoding from
//! pixels, scale so the long edge is at most `MAX_IMAGE_EDGE`, and encode lossy
//! WebP within `MAX_IMAGE_BYTES`. WebP is the only output.

use std::io::Cursor;

use image::imageops::FilterType;
use image::{DynamicImage, ImageDecoder, ImageReader, RgbaImage};
use protocol::blob_store::{MAX_IMAGE_BYTES, MAX_IMAGE_EDGE, MAX_THUMBHASH_BYTES, WEBP_QUALITY};
use protocol::photo_sticker::rgba_buffer_len;

pub use protocol::photo_sticker::SegmentationSource;

/// The longest edge a ThumbHash input may have (the crate's limit).
const THUMBHASH_EDGE: u32 = 100;

/// The quality steps tried below [`WEBP_QUALITY`] when the first encode is over
/// the byte cap.
const LOWER_QUALITIES: [f32; 4] = [70.0, 60.0, 50.0, 40.0];

/// How much each size step shrinks the edge when even the lowest quality is
/// over the cap.
const SHRINK_STEP: f32 = 0.8;

/// The smallest long edge the size steps go down to before giving up.
const MIN_EDGE: u32 = 256;

/// Why an image cannot be sent.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum ImageError {
    /// The bytes are not an image this core decodes (JPEG, PNG, GIF or WebP;
    /// HEIC and the rest are not decoded here).
    #[error("this is not an image the app can send")]
    Unsupported,
    /// Even the smallest, lowest-quality encode is over the size cap.
    #[error("the image is too large to send")]
    TooLarge,
    /// A subject cutout from a `SegmentationSource` did not meet the RGBA
    /// contract (photo-sticker: Segmentation seam and RGBA contract): the buffer
    /// was not `width`×`height`×4 bytes, or it had no non-transparent pixel.
    #[error("the subject cutout is not a valid RGBA image")]
    BadCutout,
}

/// A straight-alpha subject cutout from a [`SegmentationSource`]
/// (photo-sticker: Segmentation seam and RGBA contract). This is the **only**
/// shape that crosses the seam between a native segmentation engine and the
/// core: not a mask-only buffer, not an encoded image. Every engine, native or
/// future, hands the core this same thing, and the core styles and encodes it
/// identically regardless of which engine made it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RgbaCutout {
    /// Who produced the cutout. Carried for the record only; it does not change
    /// how the core encodes.
    pub source: SegmentationSource,
    /// Pixel width of the RGBA image.
    pub width: u32,
    /// Pixel height of the RGBA image.
    pub height: u32,
    /// Straight-alpha, 8-bit-per-channel RGBA in the sRGB color space, R,G,B,A
    /// byte order, row stride `width`×4, orientation already applied.
    /// Background pixels are fully transparent (alpha 0); subject pixels keep
    /// their original color.
    pub pixels: Vec<u8>,
}

/// An encoded image, ready to encrypt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EncodedImage {
    /// The WebP bytes, at most [`MAX_IMAGE_BYTES`].
    pub webp: Vec<u8>,
    /// Pixel width of the WebP.
    pub width: u32,
    /// Pixel height of the WebP.
    pub height: u32,
    /// The ThumbHash of the image, at most [`MAX_THUMBHASH_BYTES`] bytes.
    pub thumbhash: Vec<u8>,
}

/// Runs the whole pipeline on a picked image's bytes.
///
/// An animated input is reduced to its first frame (static only). EXIF
/// orientation is applied to the pixels and then dropped with every other
/// piece of metadata, GPS included, because the WebP is built from pixels only.
pub fn encode(input: &[u8]) -> Result<EncodedImage, ImageError> {
    let mut decoder = ImageReader::new(Cursor::new(input))
        .with_guessed_format()
        .map_err(|_| ImageError::Unsupported)?
        .into_decoder()
        .map_err(|_| ImageError::Unsupported)?;
    let orientation = decoder.orientation().ok();
    let mut image = DynamicImage::from_decoder(decoder).map_err(|_| ImageError::Unsupported)?;
    if let Some(orientation) = orientation {
        image.apply_orientation(orientation);
    }
    if image.width() == 0 || image.height() == 0 {
        return Err(ImageError::Unsupported);
    }
    let image = scale_to(&image, MAX_IMAGE_EDGE);
    let (webp, image) = fit_in_cap(image)?;
    let thumbhash = thumbhash_of(&image);
    Ok(EncodedImage {
        webp,
        width: image.width(),
        height: image.height(),
        thumbhash,
    })
}

/// Encodes a subject cutout from a `SegmentationSource` into a sticker
/// (photo-sticker: Sticker styling and encode; client-runtime: Image encode
/// pipeline). The core does the consistent work no matter which engine made the
/// mask: it trims fully-transparent margins, downscales so the long edge is at
/// most [`MAX_IMAGE_EDGE`], and encodes **WebP preserving the alpha channel**
/// within [`MAX_IMAGE_BYTES`] — never compositing onto an opaque fill — then
/// computes the ThumbHash and records the WebP `width`/`height`. The native side
/// encodes nothing and the blob key never leaves the core.
///
/// For the MVP no styling beyond trimming is applied (richer styling is future
/// work). The `cutout.source` does not change the output: every engine's cutout
/// is encoded identically.
pub fn encode_cutout(cutout: &RgbaCutout) -> Result<EncodedImage, ImageError> {
    if cutout.width == 0
        || cutout.height == 0
        || cutout.pixels.len() != rgba_buffer_len(cutout.width, cutout.height)
    {
        return Err(ImageError::BadCutout);
    }
    let rgba = RgbaImage::from_raw(cutout.width, cutout.height, cutout.pixels.clone())
        .ok_or(ImageError::BadCutout)?;
    // Trim away the fully-transparent border so the subject fills the sticker;
    // a cutout with no opaque pixel at all is not a subject.
    let rgba = trim_transparent(&rgba).ok_or(ImageError::BadCutout)?;
    let image = scale_to(&DynamicImage::ImageRgba8(rgba), MAX_IMAGE_EDGE);
    let (webp, image) = fit_in_cap(image)?;
    let thumbhash = thumbhash_of(&image);
    Ok(EncodedImage {
        webp,
        width: image.width(),
        height: image.height(),
        thumbhash,
    })
}

/// `image` cropped to the bounding box of its non-transparent pixels, keeping
/// the alpha of every pixel inside. `None` when every pixel is fully
/// transparent (no subject to cut out).
fn trim_transparent(image: &RgbaImage) -> Option<RgbaImage> {
    let (mut min_x, mut min_y, mut max_x, mut max_y) = (u32::MAX, u32::MAX, 0u32, 0u32);
    for (x, y, px) in image.enumerate_pixels() {
        if px[3] != 0 {
            min_x = min_x.min(x);
            min_y = min_y.min(y);
            max_x = max_x.max(x);
            max_y = max_y.max(y);
        }
    }
    if min_x > max_x {
        return None;
    }
    // Already tight: no copy needed.
    if (min_x, min_y, max_x, max_y) == (0, 0, image.width() - 1, image.height() - 1) {
        return Some(image.clone());
    }
    let (w, h) = (max_x - min_x + 1, max_y - min_y + 1);
    Some(image::imageops::crop_imm(image, min_x, min_y, w, h).to_image())
}

/// `image` scaled down so its long edge is at most `edge`; never up.
fn scale_to(image: &DynamicImage, edge: u32) -> DynamicImage {
    if image.width().max(image.height()) <= edge {
        return image.clone();
    }
    image.resize(edge, edge, FilterType::Lanczos3)
}

/// Encodes WebP at [`WEBP_QUALITY`], then at lower qualities, then at smaller
/// sizes, until it fits [`MAX_IMAGE_BYTES`]. Returns the bytes and the image
/// they hold.
fn fit_in_cap(mut image: DynamicImage) -> Result<(Vec<u8>, DynamicImage), ImageError> {
    loop {
        for quality in std::iter::once(WEBP_QUALITY).chain(LOWER_QUALITIES) {
            let webp = webp_of(&image, quality);
            if webp.len() <= MAX_IMAGE_BYTES {
                return Ok((webp, image));
            }
        }
        let edge = image.width().max(image.height());
        if edge <= MIN_EDGE {
            return Err(ImageError::TooLarge);
        }
        // Rounded down so the loop always ends.
        let smaller = ((edge as f32 * SHRINK_STEP) as u32).max(MIN_EDGE);
        image = scale_to(&image, smaller);
    }
}

fn webp_of(image: &DynamicImage, quality: f32) -> Vec<u8> {
    let rgba = image.to_rgba8();
    webp::Encoder::from_rgba(rgba.as_raw(), rgba.width(), rgba.height())
        .encode(quality)
        .to_vec()
}

/// The ThumbHash of `image`, from a copy of at most 100 pixels on a side.
fn thumbhash_of(image: &DynamicImage) -> Vec<u8> {
    let small = scale_to(image, THUMBHASH_EDGE).to_rgba8();
    let hash = thumbhash::rgba_to_thumb_hash(
        small.width() as usize,
        small.height() as usize,
        small.as_raw(),
    );
    debug_assert!(hash.len() <= MAX_THUMBHASH_BYTES);
    hash
}

/// The pixel size of a WebP, for checking what a download holds.
pub fn webp_size(bytes: &[u8]) -> Option<(u32, u32)> {
    let decoder = ImageReader::with_format(Cursor::new(bytes), image::ImageFormat::WebP)
        .into_decoder()
        .ok()?;
    Some(decoder.dimensions())
}

/// Whether `hash` is the ThumbHash of an image that carries transparency (some
/// pixel is not fully opaque). `false` for bytes that are not a ThumbHash.
///
/// A sticker is indistinguishable on the wire from any other transparent image
/// (photo-sticker: Sticker is an image), so the receiver uses this to render a
/// transparent image — a sticker — without an opaque bubble fill, so its
/// transparency reads (app-ui: Create a sticker). The core decodes the
/// ThumbHash so the app needs no ThumbHash code of its own.
pub fn thumbhash_is_transparent(hash: &[u8]) -> bool {
    match thumbhash::thumb_hash_to_rgba(hash) {
        Ok((_, _, rgba)) => rgba.as_chunks::<4>().0.iter().any(|px| px[3] < 255),
        Err(_) => false,
    }
}

/// The blur a ThumbHash stands for, as a small PNG the app draws until the
/// picture is downloaded (app-ui: Image bubble and viewer). `None` for bytes
/// that are not a ThumbHash. The core decodes it so the app has no ThumbHash
/// code of its own, and a PNG so the app needs no pixel API to show it.
pub fn thumbhash_preview(hash: &[u8]) -> Option<Vec<u8>> {
    let (width, height, rgba) = thumbhash::thumb_hash_to_rgba(hash).ok()?;
    let image = image::RgbaImage::from_raw(
        u32::try_from(width).ok()?,
        u32::try_from(height).ok()?,
        rgba,
    )?;
    let mut png = Vec::new();
    DynamicImage::ImageRgba8(image)
        .write_to(&mut Cursor::new(&mut png), image::ImageFormat::Png)
        .ok()?;
    Some(png)
}
