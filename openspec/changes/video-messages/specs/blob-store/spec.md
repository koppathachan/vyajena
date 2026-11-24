## MODIFIED Requirements

### Requirement: Upload
A device SHALL upload a blob with `Blobs.UploadBlob`, naming the `group_id`, a
client-generated 16-byte `blob_id` and the ciphertext. The server SHALL accept it
only from a current member device of `group_id` (`PERMISSION_DENIED` otherwise),
SHALL reject ciphertext over the server's configured maximum blob size with
`INVALID_ARGUMENT` and store nothing, and SHALL answer with the blob's `expires_at`
(upload time + `protocol::blob_store::BLOB_TTL`). The configured maximum SHALL be
read from the server's config at startup and clamped to `[the image cap,
protocol::video::MAX_VIDEO_BYTES]` (video-messages: Config-driven limit); it is one
cap for every blob, because the server sees only ciphertext bytes and cannot tell a
video blob from an image blob. Upload SHALL be idempotent on `blob_id`: a re-upload
of identical bytes SHALL return the same `expires_at`, and an upload of different
bytes under a live `blob_id` SHALL be rejected with `ALREADY_EXISTS`. The server
enforces this.

#### Scenario: Member uploads a blob
- **WHEN** Bob, a member of the group, uploads a 1.5 MB ciphertext with a fresh
  `blob_id`
- **THEN** the server stores it and returns `expires_at` seven days out

#### Scenario: Non-member upload refused
- **WHEN** a device that is not a member of `group_id` calls `Blobs.UploadBlob`
- **THEN** the server rejects it with `PERMISSION_DENIED` and stores nothing

#### Scenario: Idempotent retry
- **WHEN** Bob re-uploads the identical ciphertext under the same `blob_id`
- **THEN** the server stores no second copy and returns the same `expires_at`

#### Scenario: Over the configured cap
- **WHEN** a device uploads ciphertext larger than the server's configured maximum
  blob size
- **THEN** the server rejects it with `INVALID_ARGUMENT` and stores nothing
