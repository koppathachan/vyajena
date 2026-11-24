## 1. Proto

- [x] 1.1 `dms.proto`: `Dms.RemoveClosedDm`; `messaging.proto`:
      `ServerFrame.conversation_removed = 11` (`ConversationRemoved`); `buf lint`,
      `buf breaking`; `cargo run -p pbgen`

## 2. Server

- [x] 2.1 Migration `0008_removed_conversations.sql`; `Dms.RemoveClosedDm`
      (closed DM of the caller's user, else `PERMISSION_DENIED`; idempotent); send
      the set on connect as `conversation_removed`; tests
      `message-deletion/removed-conversation-sync`

## 3. Core

- [x] 3.1 `Client::remove_closed_dm(chat)` (closed DMs only; RPC first, then delete
      the chat row, messages and read mark); apply `conversation_removed`; tests
      `message-deletion/remove-a-closed-dm`,
      `message-deletion/removed-conversation-sync`

## 4. Bridge

- [x] 4.1 `remove_closed_dm`; `flutter_rust_bridge_codegen generate`; bridge test

## 5. App

- [x] 5.1 Confirm dialog on the closed-DM swipe, then `removeClosedDm`; drop the
      "coming soon" no-op; widget tests `app-ui/remove-a-closed-dm-from-the-list`

## 6. Check

- [x] 6.1 `cargo test --workspace`, clippy, fmt, `buf lint`, `buf breaking`,
      `flutter analyze`, `flutter test`, `openspec validate remove-closed-dm
      --strict`; manual QA on the iOS simulator and the Android emulator
