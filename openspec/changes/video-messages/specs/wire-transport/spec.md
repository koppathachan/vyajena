## MODIFIED Requirements

### Requirement: Size limits
The ciphertext of an `APPLICATION` envelope SHALL NOT exceed 65,536 bytes.
Handshake envelopes (`COMMIT` and `PROPOSAL`) SHALL NOT exceed 262,144 bytes,
encoded as sent; for a `COMMIT` this includes the Welcome it carries. The client
SHALL refuse to send an envelope over its limit, and the server SHALL reject one
with `TOO_LARGE`. The server SHALL advertise both limits in `GetServerInfo`, and
SHALL also advertise there, in `ServerInfo.Limits.max_video_blob_bytes`, its
configured maximum blob size, so clients enforce the video size limit client-side
(video-messages: Config-driven limit).

#### Scenario: Oversized application message
- **WHEN** the server receives an `APPLICATION` envelope whose ciphertext is 65,537
  bytes
- **THEN** the server rejects it with reason `TOO_LARGE` and stores nothing

#### Scenario: Commit with a large Welcome
- **WHEN** a `COMMIT` envelope is 200,000 bytes and its Welcome bundle adds
  70,000 bytes
- **THEN** the server rejects the envelope, Welcome included, with `TOO_LARGE`

#### Scenario: Client-side check
- **WHEN** encrypting a message would produce more than 65,536 bytes of ciphertext
- **THEN** the Rust core returns an error to the UI and sends nothing

#### Scenario: Video size advertised
- **WHEN** a client calls `GetServerInfo`
- **THEN** `ServerInfo.Limits` carries `max_video_blob_bytes`, and the client refuses
  a video whose encrypted blob would exceed it
