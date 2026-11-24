# Tasks

Three independent fixes. No proto/wire change. Builds serialise on the shared
external-disk cargo target. Tick a task only when its tests pass.

## 1. Typed invite (app-ui: A typed invite is accepted)

- [ ] 1.1 `app/lib/onboarding/onboarding_screen.dart`: the invite `TextField` gets
  `autocorrect: false`, `enableSuggestions: false`,
  `textCapitalization: TextCapitalization.none`.
- [ ] 1.2 `bridge/src/api/text.rs`: `payload_bytes` strips the prefix
  case-insensitively (payload stays case-sensitive).
- [ ] 1.3 Bridge test: `Vyajena-invite:<payload>` parses the same as
  `vyajena-invite:<payload>`; a non-invite prefix still reports its kind.

## 2. Dedup pending invites (device-identity: At most one pending invite per username)

- [ ] 2.1 `server/src/admin/invites.rs`: add `usable_for_username` (unused,
  unrevoked, unexpired, non-bootstrap invite for a username).
- [ ] 2.2 `server/src/admin/mod.rs::create_invite`: before creating, return the
  existing usable invite (and rebuild its payload) when one exists.
- [ ] 2.3 Server test (`server/tests/`, `// spec:
  device-identity/at-most-one-pending-invite-per-username`): two `CreateInvite` for
  the same username return the same invite; a fresh one after use is unaffected.

## 3. Media hang (client-runtime: Media the server will not store fails promptly)

- [ ] 3.1 `client/src/engine/uploads.rs`: `classify` distinguishes `UNAVAILABLE`;
  the retry state counts consecutive `UNAVAILABLE`; after
  `MAX_BLOB_UNAVAILABLE_RETRIES` the upload fails with a clear reason.
- [ ] 3.2 Non-`UNAVAILABLE` retryable statuses reset the counter and keep today's
  policy.
- [ ] 3.3 Core test (`client/tests/`, `// spec:
  client-runtime/media-the-server-will-not-store-fails-promptly`): persistent
  `UNAVAILABLE` fails the message within the bound; a brief one still uploads.

## 4. Validate / verify

- [ ] 4.1 `cargo test --workspace`, clippy, fmt; `cd app && flutter analyze &&
  flutter test`.
- [ ] 4.2 `openspec validate onboarding-media-fixes --strict` passes.
- [ ] 4.3 Simulator (coordinate with the live test session): type an invite with a
  capitalized prefix → valid; create an invite twice → one pending; send a voice
  message to a no-blob-store server → a failed state appears, not an endless
  hourglass.
