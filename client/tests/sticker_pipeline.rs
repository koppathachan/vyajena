//! The sticker encode pipeline (change `photo-sticker`; client-runtime: Image
//! encode pipeline, Sticker styling and encode; photo-sticker: Segmentation seam
//! and RGBA contract): what the core turns a `SegmentationSource`'s RGBA cutout
//! into before anything is sent. Pure: no server, no native code.

use client::media::ImageError;
use client::media::encode::{RgbaCutout, SegmentationSource, encode_cutout};
use image::ImageFormat;
use protocol::blob_store::{MAX_IMAGE_BYTES, MAX_IMAGE_EDGE, MAX_THUMBHASH_BYTES};

/// A cutout: an opaque, coloured **disc** of diameter `subject` centred in a
/// `w`×`h` canvas whose every other pixel is fully transparent, per the RGBA
/// contract (straight alpha, R,G,B,A, stride w*4). A disc, not a rectangle, so
/// that even after the core trims to the subject's bounding box the corners
/// stay transparent — as a real irregular cutout does.
fn cutout(source: SegmentationSource, w: u32, h: u32, subject: u32) -> RgbaCutout {
    let (cx, cy) = (w as f32 / 2.0, h as f32 / 2.0);
    let r = subject as f32 / 2.0;
    let mut pixels = vec![0u8; (w * h * 4) as usize];
    for y in 0..h {
        for x in 0..w {
            let (dx, dy) = (x as f32 + 0.5 - cx, y as f32 + 0.5 - cy);
            if dx * dx + dy * dy <= r * r {
                let i = ((y * w + x) * 4) as usize;
                pixels[i] = 200; // R
                pixels[i + 1] = 40; // G
                pixels[i + 2] = 80; // B
                pixels[i + 3] = 255; // A: opaque subject
            }
        }
    }
    RgbaCutout {
        source,
        width: w,
        height: h,
        pixels,
    }
}

fn decode(webp: &[u8]) -> image::RgbaImage {
    image::load_from_memory_with_format(webp, ImageFormat::WebP)
        .expect("decode webp")
        .to_rgba8()
}

fn is_webp(bytes: &[u8]) -> bool {
    bytes.len() > 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WEBP"
}

// spec: client-runtime/image-encode-pipeline
#[test]
fn a_cutout_keeps_its_transparency() {
    // A 200-px subject on a 400x400 canvas, 100-px transparent border each side.
    let sent =
        encode_cutout(&cutout(SegmentationSource::IosVision, 400, 400, 200)).expect("encode");
    assert!(is_webp(&sent.webp), "WebP is the only output");
    assert!(sent.webp.len() <= MAX_IMAGE_BYTES);
    assert!(!sent.thumbhash.is_empty() && sent.thumbhash.len() <= MAX_THUMBHASH_BYTES);

    let out = decode(&sent.webp);
    assert_eq!(out.dimensions(), (sent.width, sent.height));
    // The subject survives as a nearly-opaque, reddish pixel in the middle.
    let centre = out.get_pixel(sent.width / 2, sent.height / 2);
    assert!(centre[3] > 200, "subject stays opaque: {centre:?}");
    assert!(
        centre[0] > centre[1] && centre[0] > centre[2],
        "its colour: {centre:?}"
    );
    // Some pixel is fully (or nearly) transparent: the background reads through,
    // never composited onto an opaque fill.
    assert!(
        out.pixels().any(|px| px[3] < 20),
        "the background is transparent"
    );
}

// spec: client-runtime/sticker-styling-and-encode
#[test]
fn every_engine_is_encoded_identically() {
    // The same RGBA from different sources encodes to byte-for-byte the same
    // sticker: the source never changes the output.
    let ios = encode_cutout(&cutout(SegmentationSource::IosVision, 300, 300, 160)).expect("ios");
    let android =
        encode_cutout(&cutout(SegmentationSource::AndroidMlKit, 300, 300, 160)).expect("android");
    let future =
        encode_cutout(&cutout(SegmentationSource::OnnxLocal, 300, 300, 160)).expect("future");
    assert_eq!(ios.webp, android.webp);
    assert_eq!(ios.webp, future.webp);
    assert_eq!((ios.width, ios.height), (android.width, android.height));
}

// spec: photo-sticker/segmentation-seam
#[test]
fn transparent_margins_are_trimmed_to_the_subject() {
    // A 100-px subject in a 1000x1000 canvas is almost all transparent border;
    // the core trims to the subject's box, so the sticker is ~100x100, not 1000.
    let sent = encode_cutout(&cutout(SegmentationSource::IosVision, 1000, 1000, 100)).expect("ok");
    assert!(
        sent.width <= 120 && sent.height <= 120,
        "trimmed to the subject: {}x{}",
        sent.width,
        sent.height
    );
}

// spec: client-runtime/sticker-styling-and-encode
#[test]
fn a_cutout_over_the_edge_limit_is_downscaled() {
    // A fully-opaque cutout larger than the edge cap scales down to it.
    let big = cutout(
        SegmentationSource::IosVision,
        MAX_IMAGE_EDGE + 800,
        1000,
        MAX_IMAGE_EDGE + 800,
    );
    let sent = encode_cutout(&big).expect("encode");
    assert_eq!(sent.width.max(sent.height), MAX_IMAGE_EDGE);
}

// spec: photo-sticker/segmentation-seam
#[test]
fn a_cutout_that_breaks_the_contract_is_refused() {
    // Buffer not width*height*4.
    assert_eq!(
        encode_cutout(&RgbaCutout {
            source: SegmentationSource::IosVision,
            width: 4,
            height: 4,
            pixels: vec![0; 10],
        }),
        Err(ImageError::BadCutout)
    );
    // Zero dimensions.
    assert_eq!(
        encode_cutout(&RgbaCutout {
            source: SegmentationSource::AndroidMlKit,
            width: 0,
            height: 0,
            pixels: Vec::new(),
        }),
        Err(ImageError::BadCutout)
    );
    // All transparent: no subject at all.
    assert_eq!(
        encode_cutout(&RgbaCutout {
            source: SegmentationSource::IosVision,
            width: 8,
            height: 8,
            pixels: vec![0; 8 * 8 * 4],
        }),
        Err(ImageError::BadCutout)
    );
}
