## ADDED Requirements

### Requirement: Upload limits
The server SHALL bound uploads beyond the size of one blob. It SHALL refuse an
upload of a new blob with `RESOURCE_EXHAUSTED`, and store nothing, when the live
blobs would then exceed `protocol::blob_store::MAX_BLOB_STORE_BYTES`, when the
uploading device already uploaded `MAX_UPLOADS_PER_DEVICE_PER_HOUR` new blobs in
the last hour, or when `MAX_CONCURRENT_UPLOADS` uploads are already in progress.
A re-upload of identical bytes (idempotent retry) SHALL NOT count against the
total or the rate. The server SHALL refuse a request larger than the service's
decode cap before buffering it: `MAX_BLOB_BYTES` plus 1 KiB for `Blobs`. The
server enforces this; a client retries `RESOURCE_EXHAUSTED` later.

#### Scenario: Store full
- **WHEN** the live blobs total `MAX_BLOB_STORE_BYTES` and Bob uploads one more
- **THEN** the server rejects it with `RESOURCE_EXHAUSTED` and stores nothing,
  and uploads work again once blobs expire

#### Scenario: Too many uploads from one device
- **WHEN** Bob's device has uploaded `MAX_UPLOADS_PER_DEVICE_PER_HOUR` new blobs
  in the last hour and uploads another
- **THEN** the server rejects it with `RESOURCE_EXHAUSTED`; Alice's device is not
  limited by Bob's count

#### Scenario: Retry does not count
- **WHEN** Bob re-uploads identical ciphertext under the same `blob_id` while his
  device is at the hourly limit
- **THEN** the server returns the same `expires_at`

#### Scenario: Too many uploads at once
- **WHEN** `MAX_CONCURRENT_UPLOADS` uploads are in progress and another arrives
- **THEN** the server rejects it with `RESOURCE_EXHAUSTED`

#### Scenario: Request far over the cap
- **WHEN** a device sends an `UploadBlob` request of 5 MiB
- **THEN** the server refuses it at decode time, before the handler runs, and
  stores nothing
