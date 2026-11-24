## ADDED Requirements

### Requirement: Video normalization in a native plugin
Video normalization SHALL run in a platform plugin (AVFoundation on iOS, MediaCodec
on Android), not in the pure-Rust core: the plugin SHALL transcode a gallery clip to
canonical H.264/MP4, downscale the long edge to the 480p target, cap the bitrate to
~1 Mbps and the size to the advertised maximum, and trim to ≤ 30 seconds; an in-app
recording SHALL already be H.264/MP4 at that target. The Rust core SHALL do only the
blob crypto. The normalized plaintext SHALL live only in the app's private,
encrypt-at-rest temporary store (the image/audio cache location) before the core
encrypts it, and SHALL be removed after. The decryption key SHALL stay in the core
and SHALL never reach Dart. This is a documented departure from the core owning every
media transform. The client enforces this.

#### Scenario: Gallery clip normalized by the plugin
- **WHEN** Alice picks and trims a gallery clip
- **THEN** the native plugin produces the canonical H.264/MP4 within the size cap and
  the core encrypts those bytes, with the key never leaving the core

#### Scenario: Plaintext not kept in the clear
- **WHEN** a video has been encrypted and sent
- **THEN** the normalized plaintext is gone from the encrypt-at-rest temp and only the
  ciphertext blob and the keyed `Content` remain
