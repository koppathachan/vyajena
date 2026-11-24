## 1. Proto

- [ ] 1.1 `devices.proto`: `ServerInfo.feedback = 5`, `FeedbackChannel{method,
      target, label}`, `enum FeedbackMethod{UNSPECIFIED, NONE, EMAIL, URL}`;
      `buf lint`, `buf breaking`; `cargo run -p pbgen` (reconcile the field number
      with self-hostable's deferred server-display-name field if that is built)

## 2. Server

- [ ] 2.1 Read an operator-configured feedback channel from config (email or URL +
      label; unset ⇒ `NONE`) and return it in `GetServerInfo`; no storage, no new
      RPC; tests `user-feedback/feedback-channel-is-operator-configured`
- [ ] 2.2 `deploy/server/` optional `feedback` config key documented as
      operator-set, never committed with a real address; `check.sh` note

## 3. Core / bridge

- [ ] 3.1 Expose `ServerInfo.feedback` to the app (core already fetches
      `ServerInfo`); validate `target` is a well-formed `mailto`/`https`, else
      treat as `NONE`; bridge accessor; `flutter_rust_bridge_codegen generate`

## 4. App

- [ ] 4.1 Send feedback entry in the Profile tab, shown only when the server
      advertises a channel; compose screen showing destination + exact payload +
      opt-in diagnostics (app version, server id, platform); hand off to the OS
      mail composer (EMAIL) or open the URL (URL); widget tests
      `app-ui/send-feedback-uses-the-servers-channel`,
      `app-ui/no-feedback-channel-hides-the-entry`
- [ ] 4.2 Guard test: no feedback email/URL constant compiled into client/app
      (mirrors self-hostable's operator-agnostic guard); test
      `user-feedback/no-feedback-destination-is-hardcoded`

## 5. Check

- [ ] 5.1 `cargo test --workspace`, clippy, fmt, `buf lint`, `buf breaking`,
      `flutter analyze`, `flutter test`, `openspec validate user-feedback
      --strict`; manual QA: a server with an email channel, one with a URL channel,
      and one with none
