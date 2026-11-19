use std::time::Duration;

use protocol::blob_store::{
    BLOB_ID_LEN, BLOB_KEY_LEN, BLOB_NONCE_LEN, BLOB_TAG_LEN, BLOB_TTL, MAX_BLOB_BYTES,
    MAX_IMAGE_BYTES, MAX_IMAGE_EDGE, MAX_THUMBHASH_BYTES, WEBP_QUALITY,
};
use protocol::wire_transport::MAX_APPLICATION_CIPHERTEXT_BYTES;

// spec: blob-store/upload
#[test]
fn a_blob_holds_the_largest_image_plus_nonce_and_tag() {
    assert_eq!(
        MAX_BLOB_BYTES,
        MAX_IMAGE_BYTES + BLOB_NONCE_LEN + BLOB_TAG_LEN
    );
    // One unary RPC carries it within gRPC's default 4 MB message size.
    const { assert!(MAX_BLOB_BYTES < 4 * 1024 * 1024) };
}

// spec: client-runtime/blob-crypto-and-send-order
#[test]
fn blob_key_and_id_sizes_match_the_cipher() {
    assert_eq!(BLOB_KEY_LEN, 32);
    assert_eq!(BLOB_ID_LEN, 16);
    assert_eq!(BLOB_NONCE_LEN, 24); // XChaCha20
    assert_eq!(BLOB_TAG_LEN, 16); // Poly1305
}

// spec: client-runtime/image-encode-pipeline
#[test]
fn image_limits_are_the_owners() {
    assert_eq!(MAX_IMAGE_BYTES, 2 * 1024 * 1024);
    assert_eq!(MAX_IMAGE_EDGE, 2048);
    assert_eq!(WEBP_QUALITY, 80.0);
}

// spec: message-content/image-messages
#[test]
fn an_image_reference_fits_the_application_ciphertext_cap() {
    // An `Image` is a few dozen bytes of ids, key and ThumbHash plus a caption
    // that is limited like text; it never nears the cap by itself.
    const {
        assert!(
            BLOB_ID_LEN + BLOB_KEY_LEN + MAX_THUMBHASH_BYTES
                < MAX_APPLICATION_CIPHERTEXT_BYTES / 100
        )
    };
}

// spec: blob-store/expiry-and-ack-delete
#[test]
fn a_blob_lives_seven_days() {
    assert_eq!(BLOB_TTL, Duration::from_secs(7 * 24 * 60 * 60));
}
