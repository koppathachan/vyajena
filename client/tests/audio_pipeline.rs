//! The voice message pipeline in the core (change `audio-messages`): a recorder's
//! WAV in, OGG/Opus mono at the voice bitrate and a waveform out, with the
//! duration and size rules (audio-messages: Recording and encoding).

use std::io::Cursor;

use client::media::audio::{AudioError, encode, is_ogg_opus};
use opus_pure::{MAX_PACKET_SAMPLES, OggOpusReader, Trim};
use protocol::audio::{AUDIO_WAVEFORM_PEAKS, MAX_AUDIO_DURATION_MS, OGG_MARKER};
use protocol::blob_store::{BLOB_NONCE_LEN, BLOB_TAG_LEN, MAX_BLOB_BYTES};

/// A 16-bit PCM WAV of `ms` of a speech-like tone whose loudness swells and
/// fades, `channels` channels at `rate` Hz.
fn wav(ms: u64, rate: u32, channels: u16) -> Vec<u8> {
    let frames = ms * u64::from(rate) / 1000;
    let mut data = Vec::new();
    for i in 0..frames {
        let t = i as f32 / rate as f32;
        let swell = (0.5 + 0.5 * (2.0 * std::f32::consts::PI * 1.5 * t).sin()).powi(2);
        let tone = (2.0 * std::f32::consts::PI * (140.0 + 20.0 * t.sin()) * t).sin();
        let sample = (0.6 * swell * tone * 32767.0) as i16;
        for _ in 0..channels {
            data.extend_from_slice(&sample.to_le_bytes());
        }
    }
    wav_of(&data, rate, channels, 16)
}

fn wav_of(data: &[u8], rate: u32, channels: u16, bits: u16) -> Vec<u8> {
    let block = channels * bits / 8;
    let mut out = Vec::new();
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data.len() as u32).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&channels.to_le_bytes());
    out.extend_from_slice(&rate.to_le_bytes());
    out.extend_from_slice(&(rate * u32::from(block)).to_le_bytes());
    out.extend_from_slice(&block.to_le_bytes());
    out.extend_from_slice(&bits.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&(data.len() as u32).to_le_bytes());
    out.extend_from_slice(data);
    out
}

/// The samples and channel count the clip decodes to, trimmed as a player does.
fn decode_ogg(ogg: &[u8]) -> (Vec<f32>, usize) {
    let mut reader = OggOpusReader::new(Cursor::new(ogg)).expect("an OGG/Opus file");
    let channels = reader.head().channel_count as usize;
    let mut decoder = reader.head().decoder(48_000).expect("decoder");
    let mut trim = Trim::new(reader.head(), 48_000, channels).expect("trim");
    let mut block = vec![0.0f32; MAX_PACKET_SAMPLES * channels];
    let mut pcm = Vec::new();
    while let Some(packet) = reader.read_packet().expect("packet") {
        let n = decoder
            .decode(&packet.data, MAX_PACKET_SAMPLES, &mut block)
            .expect("decode");
        pcm.extend_from_slice(trim.keep(&packet, &block[..n * channels]));
    }
    (pcm, channels)
}

// spec: audio-messages/recording-and-encoding
#[test]
fn a_clip_is_encoded_to_opus_in_ogg_at_the_voice_bitrate() {
    let encoded = encode(&wav(20_000, 48_000, 1)).expect("encode");
    assert!(encoded.ogg.starts_with(OGG_MARKER));
    assert!(is_ogg_opus(&encoded.ogg));
    assert_eq!(encoded.duration_ms, 20_000);

    // One mono Opus stream that decodes to the clip's length.
    let (pcm, channels) = decode_ogg(&encoded.ogg);
    assert_eq!(channels, 1);
    assert_eq!(pcm.len(), 20 * 48_000);
    assert!(pcm.iter().any(|s| s.abs() > 0.1), "the sound survived");

    // About 24 kbps: 20 s is 60 kB, so 25 to 90 kB allows for the container and
    // for the encoder spending less on a simple tone.
    let kbps = encoded.ogg.len() * 8 / 20 / 1000;
    assert!((8..=32).contains(&kbps), "{kbps} kbps");
}

// spec: audio-messages/recording-and-encoding
#[test]
fn the_waveform_holds_amplitude_peaks_with_the_loudest_at_255() {
    let encoded = encode(&wav(10_000, 48_000, 1)).expect("encode");
    assert_eq!(encoded.waveform.len(), AUDIO_WAVEFORM_PEAKS);
    assert_eq!(encoded.waveform.iter().max(), Some(&255));
    // The tone swells and fades, so the bars are not all alike.
    assert!(encoded.waveform.iter().min() < Some(&120));

    // Silence is zeros, not a divide by nothing.
    let silent = encode(&wav_of(&vec![0u8; 96_000], 48_000, 1, 16)).expect("encode silence");
    assert_eq!(silent.waveform, vec![0; AUDIO_WAVEFORM_PEAKS]);
}

// spec: audio-messages/recording-and-encoding
#[test]
fn stereo_and_other_sample_rates_become_mono_opus_of_the_same_length() {
    for (rate, channels) in [(44_100, 2), (16_000, 1), (22_050, 1), (8_000, 2)] {
        let encoded = encode(&wav(3_000, rate, channels)).expect("encode");
        let (pcm, decoded_channels) = decode_ogg(&encoded.ogg);
        assert_eq!(decoded_channels, 1, "{rate} Hz x{channels}");
        assert_eq!(encoded.duration_ms, 3_000, "{rate} Hz x{channels}");
        // The decoder runs at 48 kHz whatever was encoded.
        assert_eq!(pcm.len(), 3 * 48_000, "{rate} Hz x{channels}");
    }
}

// spec: audio-messages/recording-and-encoding
#[test]
fn the_longest_clip_is_accepted_and_fits_a_blob() {
    let encoded = encode(&wav(u64::from(MAX_AUDIO_DURATION_MS), 48_000, 1)).expect("encode");
    assert_eq!(encoded.duration_ms, MAX_AUDIO_DURATION_MS);
    assert!(encoded.ogg.len() + BLOB_NONCE_LEN + BLOB_TAG_LEN < MAX_BLOB_BYTES / 2);
}

// spec: audio-messages/recording-and-encoding
#[test]
fn a_clip_longer_than_two_minutes_is_refused() {
    let refused = encode(&wav(u64::from(MAX_AUDIO_DURATION_MS) + 1_000, 48_000, 1));
    assert_eq!(refused, Err(AudioError::TooLong));
    // Judged by the samples, whatever the rate.
    let refused = encode(&wav(121_000, 16_000, 1));
    assert_eq!(refused, Err(AudioError::TooLong));
}

// spec: audio-messages/recording-and-encoding
#[test]
fn what_is_not_a_readable_recording_is_refused() {
    assert_eq!(encode(b"hello"), Err(AudioError::Unsupported));
    assert_eq!(encode(&[]), Err(AudioError::Unsupported));
    // A WAV with no sound in it.
    assert_eq!(encode(&wav_of(&[], 48_000, 1, 16)), Err(AudioError::Empty));
    // 8-bit samples and three channels are not what the recorder is asked for.
    assert_eq!(
        encode(&wav_of(&[0; 4_800], 48_000, 1, 8)),
        Err(AudioError::Unsupported)
    );
    assert_eq!(
        encode(&wav_of(&[0; 9_600], 48_000, 3, 16)),
        Err(AudioError::Unsupported)
    );
    // The container of an image is not audio either.
    assert_eq!(
        encode(b"RIFF\0\0\0\0WEBPVP8 "),
        Err(AudioError::Unsupported)
    );
}

// spec: audio-messages/recording-and-encoding
#[test]
fn a_header_that_claims_more_than_the_file_holds_is_read_as_far_as_it_goes() {
    // A recorder killed before it fixed up its header leaves the data size as
    // it began, which is 0 or far too big.
    for claimed in [0u32, u32::MAX] {
        let mut bytes = wav(2_000, 48_000, 1);
        bytes[40..44].copy_from_slice(&claimed.to_le_bytes());
        let encoded = encode(&bytes).expect("encode");
        assert_eq!(encoded.duration_ms, 2_000, "claimed {claimed}");
    }
}

// spec: audio-messages/recording-and-encoding
#[test]
fn only_a_mono_opus_ogg_file_counts_as_a_clip() {
    let encoded = encode(&wav(1_000, 48_000, 1)).expect("encode");
    assert!(is_ogg_opus(&encoded.ogg));
    assert!(!is_ogg_opus(b""));
    assert!(!is_ogg_opus(b"OggS"));
    assert!(!is_ogg_opus(&wav(1_000, 48_000, 1)));
    assert!(!is_ogg_opus(&encoded.ogg[..30]));
}
