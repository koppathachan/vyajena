use protocol::blob_store::{MAX_IMAGE_BYTES, MAX_IMAGE_EDGE};
use protocol::photo_sticker::{RGBA_BYTES_PER_PIXEL, SegmentationSource, rgba_buffer_len};

// spec: photo-sticker/segmentation-seam
#[test]
fn the_rgba_contract_is_eight_bit_rgba_with_no_row_padding() {
    // Straight-alpha 8-bit RGBA: four bytes a pixel, R,G,B,A.
    assert_eq!(RGBA_BYTES_PER_PIXEL, 4);
    // Row stride is width*4 and there is no padding between rows.
    assert_eq!(rgba_buffer_len(10, 3), 10 * 3 * 4);
    assert_eq!(rgba_buffer_len(0, 0), 0);
    // A full-size cutout at the image edge is well within memory but far above
    // the encoded byte cap, which is why the core downscales and encodes.
    let full = rgba_buffer_len(MAX_IMAGE_EDGE, MAX_IMAGE_EDGE);
    assert!(full > MAX_IMAGE_BYTES);
}

// spec: photo-sticker/segmentation-seam
#[test]
fn the_mvp_registers_the_two_native_sources_and_reserves_the_rest() {
    assert!(SegmentationSource::IosVision.is_mvp());
    assert!(SegmentationSource::AndroidMlKit.is_mvp());
    assert!(!SegmentationSource::Server.is_mvp());
    assert!(!SegmentationSource::OnnxLocal.is_mvp());
}

// spec: photo-sticker/on-device-native-cutout
#[test]
fn every_mvp_source_runs_on_device() {
    // No MVP source uploads the photo or downloads a model; only the reserved
    // server path would leave the device.
    assert!(SegmentationSource::IosVision.is_on_device());
    assert!(SegmentationSource::AndroidMlKit.is_on_device());
    assert!(SegmentationSource::OnnxLocal.is_on_device());
    assert!(!SegmentationSource::Server.is_on_device());
}
