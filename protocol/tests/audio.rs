use protocol::audio::{
    AUDIO_BITRATE_BPS, AUDIO_SAMPLE_RATE_HZ, AUDIO_WAVEFORM_PEAKS, MAX_AUDIO_DURATION_MS,
    MAX_AUDIO_WAVEFORM_BYTES, OGG_MARKER,
};
use protocol::blob_store::{BLOB_NONCE_LEN, BLOB_TAG_LEN, MAX_BLOB_BYTES};
use protocol::wire_transport::MAX_APPLICATION_CIPHERTEXT_BYTES;

// spec: audio-messages/recording-and-encoding
#[test]
fn the_voice_limits_are_the_owners() {
    assert_eq!(MAX_AUDIO_DURATION_MS, 120_000);
    assert_eq!(AUDIO_BITRATE_BPS, 24_000);
    assert_eq!(AUDIO_SAMPLE_RATE_HZ, 48_000);
    assert_eq!(OGG_MARKER, b"OggS");
}

// spec: audio-messages/recording-and-encoding
#[test]
fn a_two_minute_clip_fits_the_blob_cap_with_room_to_spare() {
    // The clip's bytes at the voice bitrate, plus the blob's nonce and tag.
    let clip = MAX_AUDIO_DURATION_MS as usize / 1000 * AUDIO_BITRATE_BPS as usize / 8;
    let blob = clip + BLOB_NONCE_LEN + BLOB_TAG_LEN;
    assert!(blob < MAX_BLOB_BYTES / 4, "{blob} vs {MAX_BLOB_BYTES}");
}

// spec: audio-messages/audio-messages
#[test]
fn the_waveform_the_core_makes_fits_the_cap_and_the_message() {
    const { assert!(AUDIO_WAVEFORM_PEAKS <= MAX_AUDIO_WAVEFORM_BYTES) };
    // An `Audio` body is ids, a key, a size, a duration and the waveform.
    const { assert!(MAX_AUDIO_WAVEFORM_BYTES < MAX_APPLICATION_CIPHERTEXT_BYTES / 100) };
}
