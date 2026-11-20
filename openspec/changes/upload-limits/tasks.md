## 1. Proto and shared rules

- [x] 1.1 No `.proto` change; `protocol/src/blob_store.rs`: the limit constants

## 2. Spec

- [x] 2.1 `blob-store`: ADD "Upload limits"; `server-deployment`: ADD "Service resource limits"; `openspec validate upload-limits --strict`

## 3. Server

- [x] 3.1 `blobs/meta.rs`: live total and per-device recent count; `blobs/mod.rs`: concurrency permit, quota, rate
- [x] 3.2 `runtime/mod.rs`: explicit `max_decoding_message_size` per service
- [x] 3.3 Tests in `server/tests/blobs.rs` naming `blob-store/upload-limits`

## 4. Deployment

- [x] 4.1 `vyajena.service`: `MemoryHigh`, `MemoryMax`, `TasksMax`; `check.sh` verifies them; `deploy_files.rs` pins both

## 5. Verify and deploy

- [ ] 5.0 Server version 0.2.0 → 0.3.0 (`server/Cargo.toml`, `Cargo.lock`); release by tag `server-v0.3.0` (docs/ci-release.md)
- [ ] 5.1 build, test, clippy `-D warnings`, fmt, `buf lint`, shellcheck, `openspec validate --all --strict`
- [ ] 5.2 Owner's go; `deploy/server/backup.sh`, `build.sh`, `deploy.sh --no-build`, `check.sh` 0 failed, relay check
