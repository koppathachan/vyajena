# Tasks

## 1. Expose the read mark (core + bridge)

- [x] 1.1 `Store::chat_read_mark(chat) -> i64` in `client/src/store/dms.rs`:
  the stored `read_up_to`, or 0 when the chat has no `chat_reads` row.
- [x] 1.2 `Client::read_mark(chat) -> MessageId` in `client/src/client.rs`,
  forwarding to the store.
- [x] 1.3 `Core::chat_read_mark(chat_id) -> i64` in `bridge/src/api/core.rs`.
- [x] 1.4 Regenerate the Flutter bindings (`cd app && flutter_rust_bridge_codegen
  generate`) and commit `bridge/src/frb_generated.rs` and `app/lib/src/rust/`.
- [x] 1.5 Bridge test (`bridge/tests/dms.rs`, `// spec: app-ui/read-mark`): the
  read mark is 0 before any read, becomes the newest-read message's id on
  `mark_chat_read`, and does not move when a later message arrives unread.

## 2. The divider and open-on-first-unread (app)

- [x] 2.1 `CoreService.chatReadMark(chatId)` and its `RustCoreService`
  implementation; a read-mark store in `FakeCoreService` that
  `markChatRead` advances to the chat's newest message.
- [x] 2.2 In `app/lib/chat/chat_screen.dart`: snapshot the read mark in
  `initState`, hold off `_markRead` until the snapshot has loaded, place a
  "New messages" divider above the first received message past the snapshot,
  and scroll to that message on open (bottom when there is none, oldest when
  everything is unread, no divider in either of those).
- [x] 2.3 Widget tests (`app/test/chat/`, `// spec: app-ui/unread-divider`):
  divider below the last-read and above the first unread with open scrolled to
  it; no divider when nothing is new; divider stays put as a new message
  arrives; no divider when everything is unread.

## 3. Scroll-to-bottom button (app)

- [x] 3.1 A scroll-to-bottom `FloatingActionButton` in the chat screen, shown
  only while scrolled up, reusing the existing `ScrollController`.
- [x] 3.2 Widget test (`app/test/chat/`, `// spec: app-ui/scroll-to-bottom`):
  the button is hidden at the bottom, appears when scrolled up, and returns to
  the bottom and hides on tap.

## 4. Verify

- [x] 4.1 `cargo test --workspace`, `cargo clippy --workspace --all-targets --
  -D warnings`, `cargo fmt --check`, `buf lint`.
- [x] 4.2 `cd app && flutter analyze && flutter test`.
- [x] 4.3 `openspec validate unread-divider --strict`; generated files
  committed; tasks ticked.
