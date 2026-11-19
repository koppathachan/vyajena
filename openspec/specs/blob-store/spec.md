# blob-store Specification

## Purpose
TBD - created by archiving change media-blobs. Update Purpose after archive.
## Requirements
### Requirement: Blobs are opaque ciphertext
The Pi SHALL store every blob as the opaque bytes a device uploads and SHALL
never decode, inspect or decrypt them; a blob's content and its key SHALL never
reach the Pi. The Pi MAY see a blob's size, its group and its timing, the same
as for the group's envelopes. The Pi enforces this.

#### Scenario: Blob stays encrypted
- **WHEN** Alice's device uploads an image blob with `Blobs.UploadBlob`
- **THEN** the Pi stores the ciphertext unchanged and holds no key for it

### Requirement: Upload
A device SHALL upload a blob with `Blobs.UploadBlob`, naming the `group_id`, a
client-generated 16-byte `blob_id` and the ciphertext. The Pi SHALL accept it
only from a current member device of `group_id` (`PERMISSION_DENIED` otherwise),
SHALL reject ciphertext over `protocol::blob_store::MAX_BLOB_BYTES` with
`INVALID_ARGUMENT` and store nothing, and SHALL answer with the blob's
`expires_at` (upload time + `protocol::blob_store::BLOB_TTL`). Upload SHALL be
idempotent on `blob_id`: a re-upload of identical bytes SHALL return the same
`expires_at`, and an upload of different bytes under a live `blob_id` SHALL be
rejected with `ALREADY_EXISTS`. The Pi enforces this.

#### Scenario: Member uploads a blob
- **WHEN** Bob, a member of the group, uploads a 1.5 MB ciphertext with a fresh
  `blob_id`
- **THEN** the Pi stores it and returns `expires_at` seven days out

#### Scenario: Non-member upload refused
- **WHEN** a device that is not a member of `group_id` calls `Blobs.UploadBlob`
- **THEN** the Pi rejects it with `PERMISSION_DENIED` and stores nothing

#### Scenario: Idempotent retry
- **WHEN** Bob re-uploads the identical ciphertext under the same `blob_id`
- **THEN** the Pi stores no second copy and returns the same `expires_at`

#### Scenario: Over the size cap
- **WHEN** a device uploads ciphertext larger than `MAX_BLOB_BYTES`
- **THEN** the Pi rejects it with `INVALID_ARGUMENT` and stores nothing

### Requirement: Download
A device SHALL fetch a blob with `Blobs.DownloadBlob` by `group_id` and
`blob_id`, and the Pi SHALL return its ciphertext only to a current member
device of `group_id` (`PERMISSION_DENIED` otherwise). The Pi SHALL answer
`NOT_FOUND` for a blob that has expired or been deleted. The Pi enforces this.

#### Scenario: Member downloads a blob
- **WHEN** Carol, a member, downloads a stored `blob_id`
- **THEN** the Pi returns the exact ciphertext that was uploaded

#### Scenario: Expired blob
- **WHEN** a device downloads a `blob_id` whose blob has expired
- **THEN** the Pi answers `NOT_FOUND`

#### Scenario: Non-member download refused
- **WHEN** a device that is not a member of `group_id` calls
  `Blobs.DownloadBlob`
- **THEN** the Pi rejects it with `PERMISSION_DENIED`

### Requirement: Expiry and ack-delete
The Pi SHALL delete a blob when `protocol::blob_store::BLOB_TTL` (7 days) has
passed since its upload, or earlier once every current member device of its
group has sent `Blobs.AckBlob` for it. The uploading device SHALL count as
having acked its own blob. After deletion the Pi SHALL answer `NOT_FOUND` for
it. The Pi enforces this.

#### Scenario: All recipients ack
- **WHEN** every other member device has acked a blob Bob uploaded
- **THEN** the Pi deletes it before the 7 days are up

#### Scenario: Backstop expiry
- **WHEN** a member device never acks a blob and 7 days pass
- **THEN** the Pi deletes it and later downloads get `NOT_FOUND`

### Requirement: Pluggable storage backend
The Pi SHALL store blob bytes through a storage backend that stores and returns
opaque ciphertext by `blob_id` and nothing else; the MVP backend is the local
filesystem at a stable configured path. The blob metadata (id, group, size,
upload and expiry times, ack set) and all authorization, expiry and ack-delete
logic SHALL stay in the Pi's SQLite, independent of the backend, so swapping the
backend changes neither the wire protocol nor who may read a blob or when it
dies. The backend SHALL never hold a decryption key. The Pi enforces this.

#### Scenario: Backend stores opaque bytes
- **WHEN** Alice's blob is stored and later downloaded
- **THEN** the backend returns the exact ciphertext by `blob_id`, and the Pi
  decided membership and expiry from its own SQLite, not from the backend

### Requirement: Blob storage and graceful degradation
The Pi SHALL store blob bytes only through the configured storage backend, never
in the SQLite database or on the main storage, and SHALL keep the database and
keys on its main storage. When no backend is configured, or the configured
backend is unavailable (its path not mounted or not writable, or a remote store
unreachable), the Pi SHALL still start and serve all messaging, and SHALL answer
every `Blobs` RPC with `UNAVAILABLE`; it SHALL NOT write a blob to the main
storage. The Pi enforces this.

#### Scenario: Backend unavailable at start
- **WHEN** the Pi starts with the filesystem backend configured but its drive
  not mounted at the stable path
- **THEN** it serves text messaging and answers `Blobs` RPCs with `UNAVAILABLE`,
  writing no blob to the SD card

#### Scenario: Feature off
- **WHEN** the Pi runs with no blob-store backend configured
- **THEN** `Blobs.UploadBlob` and `Blobs.DownloadBlob` return `UNAVAILABLE` and
  text messaging works normally
