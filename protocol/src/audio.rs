//! audio-messages: what the server and the device core agree on for voice
//! messages (change `audio-messages`). A voice message is an Opus mono clip in an
//! OGG container stored as an encrypted blob like an image (blob-store); the
//! blob's size cap is [`crate::blob_store::MAX_BLOB_BYTES`], reused unchanged.

/// The longest clip a voice message may hold: two minutes (Recording and
/// encoding).
pub const MAX_AUDIO_DURATION_MS: u32 = 120_000;

/// The Opus target bitrate for voice, in bits per second: mono speech at 24 kbps,
/// which makes a two-minute clip about 360 kB (Recording and encoding).
pub const AUDIO_BITRATE_BPS: i32 = 24_000;

/// The sample rate the core encodes at and the recorder is asked for: Opus's native
/// 48 kHz (Recording and encoding).
pub const AUDIO_SAMPLE_RATE_HZ: u32 = 48_000;

/// What every OGG file starts with (RFC 3533 capture pattern). The core checks a
/// downloaded clip begins with it before handing it to a player (Audio messages).
pub const OGG_MARKER: &[u8; 4] = b"OggS";

/// How many amplitude peaks the core puts in a `waveform` (Audio messages).
pub const AUDIO_WAVEFORM_PEAKS: usize = 64;

/// The most bytes of waveform an `Audio` body carries: room for a finer sender
/// without letting one stuff the message (Audio messages).
pub const MAX_AUDIO_WAVEFORM_BYTES: usize = 128;
