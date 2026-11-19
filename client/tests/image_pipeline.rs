//! The image encode pipeline (change `media-blobs`; client-runtime: Image
//! encode pipeline): what the core turns a picked file into before anything is
//! sent. Pure: no server.

use std::io::Cursor;

use client::media::ImageError;
use client::media::encode::{encode, thumbhash_preview, webp_size};
use image::codecs::gif::GifEncoder;
use image::{DynamicImage, Frame, ImageFormat, Rgba, RgbaImage};
use protocol::blob_store::{MAX_IMAGE_BYTES, MAX_IMAGE_EDGE, MAX_THUMBHASH_BYTES};

/// A smooth picture: compresses well, like a sky.
fn gradient(w: u32, h: u32) -> RgbaImage {
    RgbaImage::from_fn(w, h, |x, y| {
        Rgba([
            (x * 255 / w) as u8,
            (y * 255 / h) as u8,
            ((x ^ y) & 255) as u8,
            255,
        ])
    })
}

/// Pixels with no structure at all: the worst case for the size cap.
fn noise(w: u32, h: u32) -> RgbaImage {
    let mut state = 0x9E37_79B9_7F4A_7C15u64;
    RgbaImage::from_fn(w, h, |_, _| {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        let b = state.to_le_bytes();
        Rgba([b[0], b[1], b[2], 255])
    })
}

fn png(image: &RgbaImage) -> Vec<u8> {
    let mut out = Vec::new();
    DynamicImage::ImageRgba8(image.clone())
        .write_to(&mut Cursor::new(&mut out), ImageFormat::Png)
        .expect("png");
    out
}

/// A JPEG whose EXIF carries Orientation = `orientation` and a GPS-looking
/// marker that must not survive.
fn jpeg_with_exif(image: &RgbaImage, orientation: u16, secret: &[u8]) -> Vec<u8> {
    let mut jpg = Vec::new();
    DynamicImage::ImageRgba8(image.clone())
        .to_rgb8()
        .write_to(&mut Cursor::new(&mut jpg), ImageFormat::Jpeg)
        .expect("jpeg");
    let mut tiff = vec![b'I', b'I', 42, 0, 8, 0, 0, 0, 1, 0];
    tiff.extend([0x12, 0x01, 3, 0, 1, 0, 0, 0]);
    tiff.extend(orientation.to_le_bytes());
    tiff.extend([0, 0, 0, 0, 0, 0]);
    tiff.extend(secret);
    let mut segment = b"Exif\0\0".to_vec();
    segment.extend(tiff);
    let len = (segment.len() + 2) as u16;
    let mut out = vec![0xFF, 0xD8, 0xFF, 0xE1];
    out.extend(len.to_be_bytes());
    out.extend(segment);
    out.extend(&jpg[2..]);
    out
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack.windows(needle.len()).any(|w| w == needle)
}

fn is_webp(bytes: &[u8]) -> bool {
    bytes.len() > 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WEBP"
}

// spec: client-runtime/image-encode-pipeline
#[test]
fn a_big_photo_becomes_a_small_webp_with_no_metadata_and_upright() {
    // 4000x3000 as the camera took it, with a rotate-90 flag and a GPS marker.
    let photo = jpeg_with_exif(&gradient(4000, 3000), 6, b"GPSLatitude=12.97,77.59");
    assert!(contains(&photo, b"GPSLatitude"), "the input has the marker");

    let sent = encode(&photo).expect("encode");

    assert!(is_webp(&sent.webp), "the only wire format is WebP");
    assert!(sent.webp.len() <= MAX_IMAGE_BYTES);
    // Rotated upright first (3000x4000), then scaled to 2048 on the long edge.
    assert_eq!((sent.width, sent.height), (1536, 2048));
    assert_eq!(webp_size(&sent.webp), Some((1536, 2048)));
    assert!(sent.thumbhash.len() <= MAX_THUMBHASH_BYTES && !sent.thumbhash.is_empty());
    // Nothing of the EXIF, GPS included, reached the output.
    assert!(!contains(&sent.webp, b"GPSLatitude"));
    assert!(!contains(&sent.webp, b"EXIF"));
    assert!(!contains(&sent.webp, b"Exif"));
}

// spec: client-runtime/image-encode-pipeline
#[test]
fn a_small_picture_is_not_scaled_up() {
    let sent = encode(&png(&gradient(300, 200))).expect("encode");
    assert_eq!((sent.width, sent.height), (300, 200));
    assert!(is_webp(&sent.webp));
}

// spec: client-runtime/image-encode-pipeline
#[test]
fn the_long_edge_never_exceeds_the_limit() {
    let sent = encode(&png(&gradient(MAX_IMAGE_EDGE + 500, 100))).expect("encode");
    assert_eq!(sent.width, MAX_IMAGE_EDGE);
    assert!(sent.height >= 1);
}

// spec: client-runtime/image-encode-pipeline
#[test]
fn a_picture_that_will_not_compress_still_fits_the_size_cap() {
    // 2048x2048 of noise is several MB at quality 80: the pipeline lowers the
    // quality, then the size, until it fits.
    let sent = encode(&png(&noise(2048, 2048))).expect("encode");
    assert!(
        sent.webp.len() <= MAX_IMAGE_BYTES,
        "{} bytes",
        sent.webp.len()
    );
    assert!(sent.width.max(sent.height) <= MAX_IMAGE_EDGE);
    assert_eq!(webp_size(&sent.webp), Some((sent.width, sent.height)));
}

// spec: client-runtime/image-encode-pipeline
#[test]
fn an_animated_gif_is_sent_as_its_first_frame() {
    let mut gif = Vec::new();
    {
        let mut encoder = GifEncoder::new(&mut gif);
        for red in [200u8, 10] {
            let frame = RgbaImage::from_pixel(64, 48, Rgba([red, 0, 0, 255]));
            encoder.encode_frame(Frame::new(frame)).expect("frame");
        }
    }
    let sent = encode(&gif).expect("encode");
    assert_eq!((sent.width, sent.height), (64, 48));
    let first = image::load_from_memory_with_format(&sent.webp, ImageFormat::WebP)
        .expect("decode")
        .to_rgba8();
    // Lossy, so close to the first frame's red and far from the second's.
    assert!(
        first.get_pixel(32, 24)[0] > 150,
        "{:?}",
        first.get_pixel(32, 24)
    );
}

// spec: client-runtime/image-encode-pipeline
#[test]
fn something_that_is_not_an_image_is_refused() {
    assert_eq!(encode(b"not an image at all"), Err(ImageError::Unsupported));
    assert_eq!(encode(&[]), Err(ImageError::Unsupported));
    // A truncated file is not an image either.
    let png = png(&gradient(100, 100));
    assert_eq!(encode(&png[..png.len() / 3]), Err(ImageError::Unsupported));
    // HEIC is not decoded here: the app's picker hands over JPEG or PNG.
    let heic = [&[0u8, 0, 0, 24][..], b"ftypheic", &[0; 16]].concat();
    assert_eq!(encode(&heic), Err(ImageError::Unsupported));
}

// spec: app-ui/image-bubble-and-viewer
#[test]
fn the_thumbhash_decodes_to_a_small_png_blur_of_the_same_shape() {
    let sent = encode(&png(&gradient(800, 400))).expect("encode");
    let blur = thumbhash_preview(&sent.thumbhash).expect("preview");
    assert_eq!(
        &blur[..8],
        b"\x89PNG\r\n\x1a\n",
        "a PNG the app can just draw"
    );
    let image = image::load_from_memory_with_format(&blur, ImageFormat::Png)
        .expect("decode")
        .to_rgba8();
    let (w, h) = image.dimensions();
    assert!(w.max(h) <= 64, "a blur is small: {w}x{h}");
    // The picture is twice as wide as tall; so is its blur.
    let ratio = f64::from(w) / f64::from(h);
    assert!((1.7..2.3).contains(&ratio), "{w}x{h}");
    // Not transparent, not all one colour.
    assert!(image.pixels().all(|px| px[3] == 255));
    assert!(
        image
            .pixels()
            .any(|px| px.0[..3] != image.get_pixel(0, 0).0[..3])
    );
    // Bytes that are not a ThumbHash are no preview.
    assert_eq!(thumbhash_preview(&[1, 2]), None);
    assert_eq!(thumbhash_preview(&[]), None);
}
