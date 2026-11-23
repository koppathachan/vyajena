//! audio-messages: Recording and encoding. Every audio transform lives here, in the
//! core: Dart hands over the WAV file the recorder wrote and nothing else touches
//! samples for the wire (design D4).
//!
//! Read the WAV (16-bit PCM, mono or stereo, any rate), mix it to mono, resample
//! to a rate Opus takes when it is not one, refuse a clip longer than
//! [`MAX_AUDIO_DURATION_MS`], encode Opus at [`AUDIO_BITRATE_BPS`] into an OGG
//! container (RFC 7845) and compute the waveform peaks. OGG/Opus mono is the only
//! output.

use opus_pure::{
    Application, MAX_PACKET_BYTES, OggOpusReader, OggOpusWriter, OpusEncoder, OpusHead,
};
use protocol::audio::{
    AUDIO_BITRATE_BPS, AUDIO_SAMPLE_RATE_HZ, AUDIO_WAVEFORM_PEAKS, MAX_AUDIO_DURATION_MS,
    OGG_MARKER,
};
use protocol::blob_store::{BLOB_NONCE_LEN, BLOB_TAG_LEN, MAX_BLOB_BYTES};

/// The sample rates Opus takes as they are.
const OPUS_RATES: [u32; 5] = [8_000, 12_000, 16_000, 24_000, 48_000];

/// Why a recording cannot be sent.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum AudioError {
    /// The bytes are not a WAV this core reads (16-bit PCM, one or two
    /// channels).
    #[error("this is not a recording the app can send")]
    Unsupported,
    /// There is no sound in it at all.
    #[error("the recording is empty")]
    Empty,
    /// It is longer than [`MAX_AUDIO_DURATION_MS`].
    #[error("the voice message is too long")]
    TooLong,
    /// The encoded clip would not fit in a blob.
    #[error("the voice message is too large to send")]
    TooLarge,
    /// The Opus encoder failed.
    #[error("the voice message could not be encoded")]
    Encode,
}

/// An encoded voice message, ready to encrypt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EncodedAudio {
    /// The OGG/Opus mono bytes, which fit a blob.
    pub ogg: Vec<u8>,
    /// The clip's length in milliseconds, 1 to [`MAX_AUDIO_DURATION_MS`].
    pub duration_ms: u32,
    /// The amplitude peaks, at most [`AUDIO_WAVEFORM_PEAKS`] of them, one byte
    /// each, the loudest bar being 255.
    pub waveform: Vec<u8>,
}

fn le_u16(bytes: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_le_bytes(bytes.get(at..at + 2)?.try_into().ok()?))
}

fn le_u32(bytes: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(bytes.get(at..at + 4)?.try_into().ok()?))
}

/// The mono samples of a WAV and its sample rate.
struct Pcm {
    rate: u32,
    samples: Vec<f32>,
}

/// Reads a 16-bit PCM WAV, mixing stereo down to mono. A `data` chunk that claims
/// more than the file holds (a recorder that never came back to fix the header)
/// is read as far as the file goes. A clip over [`MAX_AUDIO_DURATION_MS`] is
/// refused before its samples are copied.
fn read_wav(wav: &[u8]) -> Result<Pcm, AudioError> {
    if wav.get(..4) != Some(b"RIFF") || wav.get(8..12) != Some(b"WAVE") {
        return Err(AudioError::Unsupported);
    }
    let mut format: Option<(u16, u32)> = None;
    let mut at = 12usize;
    while let (Some(id), Some(size)) = (wav.get(at..at + 4), le_u32(wav, at + 4)) {
        let body = at + 8;
        let size = size as usize;
        if id == b"fmt " {
            let tag = le_u16(wav, body).ok_or(AudioError::Unsupported)?;
            let channels = le_u16(wav, body + 2).ok_or(AudioError::Unsupported)?;
            let rate = le_u32(wav, body + 4).ok_or(AudioError::Unsupported)?;
            let bits = le_u16(wav, body + 14).ok_or(AudioError::Unsupported)?;
            // 0xFFFE is WAVE_FORMAT_EXTENSIBLE; its sub-format starts with the
            // plain tag.
            let pcm = tag == 1 || (tag == 0xFFFE && le_u16(wav, body + 24) == Some(1));
            if !pcm || bits != 16 || !(1..=2).contains(&channels) || rate == 0 {
                return Err(AudioError::Unsupported);
            }
            format = Some((channels, rate));
        } else if id == b"data" {
            let (channels, rate) = format.ok_or(AudioError::Unsupported)?;
            let available = wav.len().saturating_sub(body);
            let len = if size == 0 || size > available {
                available
            } else {
                size
            };
            let data = wav.get(body..body + len).ok_or(AudioError::Unsupported)?;
            return pcm_of(data, channels, rate);
        }
        // Chunks are padded to an even length.
        at = body.saturating_add(size).saturating_add(size & 1);
    }
    Err(AudioError::Unsupported)
}

fn pcm_of(data: &[u8], channels: u16, rate: u32) -> Result<Pcm, AudioError> {
    let width = 2 * usize::from(channels);
    let frames = data.len() / width;
    if frames == 0 {
        return Err(AudioError::Empty);
    }
    // Before copying anything: the longest clip is a fixed number of frames.
    if frames as u64 * 1000 > u64::from(MAX_AUDIO_DURATION_MS) * u64::from(rate) {
        return Err(AudioError::TooLong);
    }
    let sample = |bytes: &[u8]| f32::from(i16::from_le_bytes([bytes[0], bytes[1]])) / 32768.0;
    let samples = data
        .chunks_exact(width)
        .map(|frame| {
            if channels == 1 {
                sample(frame)
            } else {
                (sample(&frame[..2]) + sample(&frame[2..4])) / 2.0
            }
        })
        .collect();
    Ok(Pcm { rate, samples })
}

/// Linear resampling to `to` Hz, good enough for speech headed into a 24 kbps
/// Opus stream.
fn resample(pcm: Pcm, to: u32) -> Pcm {
    if pcm.rate == to {
        return pcm;
    }
    let out_len = (pcm.samples.len() as u64 * u64::from(to) / u64::from(pcm.rate)) as usize;
    let step = f64::from(pcm.rate) / f64::from(to);
    let last = pcm.samples.len().saturating_sub(1);
    let samples = (0..out_len)
        .map(|i| {
            let at = i as f64 * step;
            let from = (at as usize).min(last);
            let (a, b) = (pcm.samples[from], pcm.samples[(from + 1).min(last)]);
            a + (b - a) * (at - from as f64) as f32
        })
        .collect();
    Pcm { rate: to, samples }
}

/// The waveform: [`AUDIO_WAVEFORM_PEAKS`] evenly spaced buckets (fewer for a
/// clip of fewer samples), each the loudest sample in it, scaled so the loudest
/// of the clip is 255. Silence is all zeros.
fn waveform(samples: &[f32]) -> Vec<u8> {
    let peaks = samples.len().min(AUDIO_WAVEFORM_PEAKS);
    if peaks == 0 {
        return Vec::new();
    }
    let bucket = |i: usize| samples.len() * i / peaks..samples.len() * (i + 1) / peaks;
    let loud: Vec<f32> = (0..peaks)
        .map(|i| {
            samples[bucket(i)]
                .iter()
                .fold(0.0_f32, |peak, s| peak.max(s.abs()))
        })
        .collect();
    let top = loud.iter().copied().fold(0.0_f32, f32::max);
    if top <= 0.0 {
        return vec![0; peaks];
    }
    loud.iter()
        .map(|peak| (peak / top * 255.0).round() as u8)
        .collect()
}

/// Runs the whole pipeline on the WAV a recorder wrote.
pub fn encode(wav: &[u8]) -> Result<EncodedAudio, AudioError> {
    let mut pcm = read_wav(wav)?;
    if !OPUS_RATES.contains(&pcm.rate) {
        pcm = resample(pcm, AUDIO_SAMPLE_RATE_HZ);
    }
    let rate = pcm.rate;
    let duration_ms = (pcm.samples.len() as u64 * 1000 / u64::from(rate)).max(1) as u32;
    if duration_ms > MAX_AUDIO_DURATION_MS {
        return Err(AudioError::TooLong);
    }
    let ogg = encode_opus(&pcm.samples, rate)?;
    // The blob is the nonce, the clip and the tag.
    if ogg.len() + BLOB_NONCE_LEN + BLOB_TAG_LEN > MAX_BLOB_BYTES {
        return Err(AudioError::TooLarge);
    }
    Ok(EncodedAudio {
        ogg,
        duration_ms,
        waveform: waveform(&pcm.samples),
    })
}

/// Mono samples to OGG/Opus at the voice bitrate, ending exactly where the audio
/// does (RFC 7845 §4.4: the last page's granule position trims the padding).
fn encode_opus(samples: &[f32], rate: u32) -> Result<Vec<u8>, AudioError> {
    let encode = || -> opus_pure::Result<Vec<u8>> {
        let frame = rate as usize / 50;
        let mut encoder = OpusEncoder::new(rate as i32, 1, Application::Voip)?;
        encoder.bitrate_bps = AUDIO_BITRATE_BPS;
        let head = OpusHead::for_encoder(&encoder, rate);
        // Every Opus rate divides 48 kHz, so the granule arithmetic is exact.
        let ticks = 48_000 / rate as usize;
        // The encoder runs `pre_skip` behind: feed that much silence after the
        // audio to flush it, then round up to whole frames.
        let frames = (samples.len() + (head.pre_skip as usize).div_ceil(ticks)).div_ceil(frame);
        let final_granule = u64::from(head.pre_skip) + (samples.len() * ticks) as u64;
        let mut writer = OggOpusWriter::new(Vec::new(), head)?;
        let mut packet = vec![0u8; MAX_PACKET_BYTES];
        let mut block = vec![0.0f32; frame];
        for i in 0..frames {
            let start = (i * frame).min(samples.len());
            let end = (start + frame).min(samples.len());
            block[..end - start].copy_from_slice(&samples[start..end]);
            block[end - start..].fill(0.0);
            let n = encoder.encode(&block, frame, &mut packet)?;
            if i + 1 == frames {
                let counted = writer.granule().max(0) as u64;
                let last = final_granule.saturating_sub(counted);
                writer.write_packet_with_duration(&packet[..n], last as u32)?;
            } else {
                writer.write_packet(&packet[..n])?;
            }
        }
        writer.finish()
    };
    encode().map_err(|_| AudioError::Encode)
}

/// Whether `bytes` is an OGG file holding one mono Opus stream: what a downloaded
/// clip must be before the core caches it and a player is handed it (Audio
/// messages). Reads the headers only.
pub fn is_ogg_opus(bytes: &[u8]) -> bool {
    bytes.starts_with(OGG_MARKER)
        && OggOpusReader::new(std::io::Cursor::new(bytes))
            .is_ok_and(|reader| reader.head().channel_count == 1)
}
