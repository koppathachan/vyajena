## 0. Before starting

- [ ] 0.1 Owner approves the durability-invariant clarification (an authorised
      sender or admin may retract undelivered copies, always with a delivered
      tombstone): approved 2026-11-20
- [ ] 0.2 `media-blobs`, `dms` and `lobby-multi-group` merged, for the full
      delete matrix (the core mechanism does not need them)

## 1. Spikes

- [x] 1.1 S1: the server can delete a target's queue entries from `(group_id,
      target_sender, target_msg_id)` with no cursor/ordering breakage, racing ack
      harmless; result in design D4 (done 2026-11-20, by reading the store schema)

## 2. Proto and shared rules

- [ ] 2.1 `content.proto`: `Delete` body in the `Content` oneof (field number TBD —
      coordinate with `audio-messages`/`video-messages`/`audio-calls`);
      `messaging.proto`: `Envelope.deletes` (`DeleteTarget`, field number TBD);
      `blobs.proto`: `Blobs.DeleteBlob`; `buf lint`, `buf breaking`;
      `cargo run -p pbgen`
- [ ] 2.2 `protocol`: the delete authorization rule (an admin of a lobby/named
      group; a DM only the target's sender); tests `message-deletion/who-may-delete`

## 3. Server

- [ ] 3.1 On an `APPLICATION` envelope with `deletes` set: authorise it, else
      `POLICY_VIOLATION`; tests `message-deletion/who-may-delete`
- [ ] 3.2 Drop the target's undelivered `APPLICATION` queue entries; deliver the
      Delete envelope; target need not exist; tests
      `message-delivery/deletion-retracts-undelivered-copies`,
      `message-deletion/tombstone-not-erasure`
- [ ] 3.3 `Blobs.DeleteBlob`: record the uploader per blob; authorise (admin of the
      group, or the uploader in a DM) else `PERMISSION_DENIED`; purge the one copy;
      no-op on an absent blob; tests `blob-store/delete-a-blob`

## 4. Core

- [ ] 4.1 `Delete` body encode/decode; set `deletes` on the sent envelope; the
      per-chat delete-right check before sending; `delete_message(chat, id)` API
- [ ] 4.2 Apply a received `Delete` only from an authorised MLS sender; mark the
      target deleted; keep the original plaintext for a DM reveal, drop it for a
      lobby/named-group delete; a `ChatBody::Deleted`; `reveal(chat, id)` for a DM;
      events; tests `message-deletion/tombstone-not-erasure`,
      `message-deletion/receiver-may-reveal-in-a-dm`
- [ ] 4.3 A reply quoting a deleted message shows a "deleted message" quote; a media
      delete calls `Blobs.DeleteBlob` to purge the blob at once after sending the
      `Delete`; delete allowed after expiry (no-op purge); tests
      `message-deletion/deleting-media-and-replies`, `blob-store/delete-a-blob`

## 5. Bridge

- [ ] 5.1 `ChatBody::Deleted`, `delete_message`, `reveal`;
      `flutter_rust_bridge_codegen generate`; bridge end-to-end test

## 6. App

- [ ] 6.1 A Delete action in the long-press sheet (admin device only in a
      lobby/named group, on any message; in a DM only the user's own message); the
      tombstone bubble ("This message was deleted", "by the admin"); widget tests
      `app-ui/deleting-a-message`
- [ ] 6.2 The DM Reveal affordance on a tombstone; widget tests
      `app-ui/deleting-a-message`

## 7. Docs and check

- [ ] 7.1 `docs/architecture-decisions.md` (Scope "no edit or delete" → "delete,
      no edit"; the durability clarification), `openspec/config.yaml` context
- [ ] 7.2 `cargo test --workspace`, clippy, fmt, `buf lint`, `buf breaking`,
      `flutter analyze`, `flutter test`, `openspec validate delete-messages
      --strict`
