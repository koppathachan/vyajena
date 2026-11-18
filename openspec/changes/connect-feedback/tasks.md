## 1. Spec and design

- [x] 1.1 Proposal, design (D1–D3) and the app-ui `Foreground and background` delta; `openspec validate connect-feedback --strict`

## 2. App: the indicator

- [x] 2.1 `connection_indicator.dart`: pulse the connecting glyph (repeating while connecting only, stopped otherwise), give connecting the active `primary` colour distinct from the muted offline colour; a `color(scheme, status, removed:)` helper
- [x] 2.2 `connection_indicator.dart`: tap-to-retry — an `InkResponse`/`Semantics` button that calls `reconnectNow()` while connecting or offline (not removed); no-op while connected or removed; keep the tooltip, icon and label helpers other screens' tests use

## 3. App: pull-to-refresh

- [x] 3.1 `chat_list_screen.dart`: a `RefreshIndicator` around the chat list that calls `reconnectNow()`

## 4. Tests

- [x] 4.1 `app/test/app/connection_indicator_test.dart` (tagged `// spec: app-ui/foreground-and-background`): the three visual states (connecting animates in the active colour; offline static and muted; connected static), and that connecting's colour ≠ offline's
- [x] 4.2 Same file: tap while offline calls `reconnectNow`; tap while connected and while removed calls nothing; pull-to-refresh on the chat list calls `reconnectNow`

## 5. Check

- [x] 5.1 `cd app && flutter analyze` clean, `flutter test` green (including the existing app-bar tests), `openspec validate connect-feedback --strict`
