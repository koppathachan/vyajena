## 1. Proto

- [x] 1.1 New `prefs.proto`: `Prefs.SetMutePref` (`SetMutePrefRequest{scope,
      group_id, muted}`, `MuteScope`); `messaging.proto`:
      `ServerFrame.mute_prefs = 12` (`MutePrefs{muted_group_ids, muted_all}`);
      `buf lint`, `buf breaking`; `cargo run -p pbgen`

## 2. Server

- [x] 2.1 Migration `0009_mute_prefs.sql` (`muted_conversations(username,
      group_id)` + per-user `muted_all`; ids and flag only); `Prefs.SetMutePref`
      (member-of-conversation else `PERMISSION_DENIED`; global scope sets the
      flag; idempotent); send the mute set on connect as `mute_prefs`; tests
      `notification-prefs/mute-preference-sync`
- [x] 2.2 Wake-push sender consults the recipient user's mute set and skips the
      push for a muted conversation or when `muted_all`; message still queued and
      delivered; test
      `notification-prefs/a-muted-conversation-gets-no-wake-push`

## 3. Core

- [x] 3.1 `Client::set_mute_pref(target, muted)` (RPC; keep a local mute set);
      apply `mute_prefs` snapshot; suppress the local in-app notification/badge
      for a muted conversation; expose the mute set to the app; tests
      `notification-prefs/mute-a-conversation`,
      `notification-prefs/mute-preference-sync`

## 4. Bridge

- [x] 4.1 `set_mute_pref` + a mute-set accessor; `flutter_rust_bridge_codegen
      generate`; bridge test

## 5. App

- [x] 5.1 Mute/unmute on a conversation (overflow menu + chat-row long-press) and
      a global "mute all" in the Profile tab; mute icon on muted rows; whole-app
      mute shown as overriding; widget tests
      `app-ui/mute-a-conversation-from-the-chat-list`,
      `app-ui/mute-the-whole-app`

## 6. Check

- [ ] 6.1 `cargo test --workspace`, clippy, fmt, `buf lint`, `buf breaking`,
      `flutter analyze`, `flutter test`, `openspec validate mute-notifications
      --strict`; manual QA on the iOS simulator and the Android emulator
