## Why

A 2 MiB cap per image exists (`MAX_BLOB_BYTES`), but nothing bounds how much a
member can upload in total, how fast, or how many uploads the server buffers at
once, and the unit has no memory ceiling. On a small host a buggy or abusive
member device could fill the blob drive or exhaust memory (owner, 2026-11-20:
"it could crash the server").

## What Changes

- **Total blob store cap:** the server SHALL refuse an upload that would take the
  live blobs over `MAX_BLOB_STORE_BYTES` (1 GiB) with `RESOURCE_EXHAUSTED`.
- **Per-device rate:** at most `MAX_UPLOADS_PER_DEVICE_PER_HOUR` (60) new blobs per
  device per hour, `RESOURCE_EXHAUSTED` beyond.
- **Concurrency:** at most `MAX_CONCURRENT_UPLOADS` (8) uploads in progress at
  once (each is buffered whole in memory); one more gets `RESOURCE_EXHAUSTED`.
- **Message size cap at decode:** each service's `max_decoding_message_size` is set
  explicitly (blobs: `MAX_BLOB_BYTES` + 1 KiB; every other service 1 MiB), so an
  oversized request is refused before it is buffered, instead of relying on
  tonic's 4 MiB default.
- **Unit limits:** `vyajena.service` gets `MemoryHigh=384M`, `MemoryMax=512M` and
  `TasksMax=256`; the hardening check verifies them.
- No `.proto` change (`RESOURCE_EXHAUSTED` is an ordinary gRPC status). The core
  already retries it (client `classify`), so a limited device tries again later.

## Capabilities

### Modified Capabilities

- `blob-store`: new requirement "Upload limits".
- `server-deployment`: new requirement "Service resource limits".

## Impact

- Code: `protocol/src/blob_store.rs` (constants), `server/src/blobs/mod.rs`
  (limits), `server/src/blobs/meta.rs` (two queries), `server/src/runtime/mod.rs`
  (decode caps), `deploy/server/vyajena.service`, `deploy/server/check.sh`.
- Tests: `server/tests/blobs.rs`, `server/tests/deploy_files.rs`.
- The numbers are policy defaults (design D1); the owner can change them in one
  place, `protocol/`.
- Deploys with the next server build; no data migration.
