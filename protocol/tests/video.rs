use protocol::blob_store::{BLOB_NONCE_LEN, BLOB_TAG_LEN, MAX_BLOB_BYTES, MAX_THUMBHASH_BYTES};
use protocol::video::{
    CONFIG_KEY, DEFAULT_MAX_VIDEO_BYTES, MAX_VIDEO_BYTES, MAX_VIDEO_DURATION_MS, MP4_FTYP_MARKER,
    NORMALIZE, blob_size, clamp_max_blob_bytes, max_upload_request_bytes,
};

// spec: video-messages/config-driven-limit
#[test]
fn the_video_limits_are_the_owners() {
    // The 30-second cap the client enforces on duration (owner, 2026-11-23).
    assert_eq!(MAX_VIDEO_DURATION_MS, 30_000);
    // The ceiling the server clamps the configured cap to, and the default it
    // uses when unset or out of range. The default is below the ceiling so a
    // generous but bounded cap is in force out of the box.
    assert_eq!(MAX_VIDEO_BYTES, 8 * 1024 * 1024);
    assert_eq!(DEFAULT_MAX_VIDEO_BYTES, 5 * 1024 * 1024);
    assert!(DEFAULT_MAX_VIDEO_BYTES < MAX_VIDEO_BYTES);
    assert_eq!(CONFIG_KEY, "max_video_blob_bytes");
}

// spec: video-messages/config-driven-limit
#[test]
fn the_normalize_target_is_480p_at_about_one_mbps() {
    // 854 is the long edge of 480p (16:9); the bitrate is about 1 Mbps (owner).
    assert_eq!(NORMALIZE.long_edge, 854);
    assert_eq!(NORMALIZE.video_bitrate_bps, 1_000_000);
}

// spec: video-messages/config-driven-limit
#[test]
fn an_absent_config_falls_back_to_the_default() {
    assert_eq!(clamp_max_blob_bytes(None), DEFAULT_MAX_VIDEO_BYTES as u64);
}

// spec: video-messages/config-driven-limit
#[test]
fn a_config_in_range_is_used_as_is() {
    let value = 6 * 1024 * 1024;
    assert_eq!(clamp_max_blob_bytes(Some(value)), value);
}

// spec: video-messages/config-driven-limit
#[test]
fn a_config_below_the_image_cap_is_clamped_up_to_it() {
    // The one cap also governs image blobs, so it can never drop below the image
    // cap (blob-store: Upload).
    assert_eq!(clamp_max_blob_bytes(Some(1)), MAX_BLOB_BYTES as u64);
    assert_eq!(
        clamp_max_blob_bytes(Some(MAX_BLOB_BYTES as u64 - 1)),
        MAX_BLOB_BYTES as u64
    );
}

// spec: video-messages/config-driven-limit
#[test]
fn a_config_above_the_ceiling_is_clamped_down_to_it() {
    // Scenario "Config beyond the ceiling": clamp to MAX_VIDEO_BYTES, not reject.
    assert_eq!(
        clamp_max_blob_bytes(Some(MAX_VIDEO_BYTES as u64 + 1)),
        MAX_VIDEO_BYTES as u64
    );
    assert_eq!(
        clamp_max_blob_bytes(Some(u64::MAX)),
        MAX_VIDEO_BYTES as u64
    );
}

// spec: video-messages/config-driven-limit
#[test]
fn the_image_cap_is_a_valid_floor_below_the_ceiling() {
    // The clamp interval is non-empty: the image cap sits below the ceiling.
    assert!(MAX_BLOB_BYTES < MAX_VIDEO_BYTES);
}

// spec: video-messages/config-driven-limit
#[test]
fn the_upload_request_limit_leaves_room_for_the_ids() {
    let cap = clamp_max_blob_bytes(Some(6 * 1024 * 1024));
    assert_eq!(max_upload_request_bytes(cap), cap as usize + 1024);
    // It is always called with a clamped value; at the ceiling it stays the
    // ceiling plus the id headroom.
    assert_eq!(
        max_upload_request_bytes(MAX_VIDEO_BYTES as u64),
        MAX_VIDEO_BYTES + 1024
    );
}

// spec: video-messages/video-messages
#[test]
fn a_blob_is_the_plaintext_plus_the_nonce_and_tag() {
    assert_eq!(blob_size(1000), (1000 + BLOB_NONCE_LEN + BLOB_TAG_LEN) as u64);
}

// spec: video-messages/video-messages
#[test]
fn a_first_frame_thumbhash_shares_the_image_thumbhash_cap() {
    // The video poster reuses the image ThumbHash budget; it is small (~25 bytes).
    assert_eq!(MP4_FTYP_MARKER, b"ftyp");
    assert!(MAX_THUMBHASH_BYTES >= 25);
}
