//! client-runtime: Image encode pipeline. Every image transform lives here, in
//! the core: Dart hands over the picked bytes and nothing else touches pixels
//! for the wire (design D5).
//!
//! Decode, apply the EXIF orientation, drop all metadata by encoding from
//! pixels, scale so the long edge is at most `MAX_IMAGE_EDGE`, and encode lossy
//! WebP within `MAX_IMAGE_BYTES`. WebP is the only output.

use std::io::Cursor;

use image::imageops::FilterType;
use image::{DynamicImage, ImageDecoder, ImageReader};
use protocol::blob_store::{MAX_IMAGE_BYTES, MAX_IMAGE_EDGE, MAX_THUMBHASH_BYTES, WEBP_QUALITY};

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
