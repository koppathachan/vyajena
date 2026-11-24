## 1. Server — re-ping on stream loss

- [ ] 1.1 On `Connect` stream close / idle-timeout reap, if the device still has
      undelivered queued entries and a push token, request a wake push (shared
      one-per-device-per-60 s limit); test
      `message-delivery/re-ping-on-stream-loss`

## 2. Server — sweep catch-up

- [ ] 2.1 In `sweep.rs`, for each offline device with undelivered backlog and a
      token, request a wake push (rate-limited), repeating per sweep until the
      backlog drains or `CATCH_UP_MAX` attempts; reset the attempt counter when the
      queue drains; respect the mute check (mute-notifications); tests
      `message-delivery/sweep-catch-up-ping`,
      `message-delivery/catch-up-stops-when-drained`

## 3. Client — prompt reconnection

- [ ] 3.1 Reconnect policy: re-open `Connect` on foreground, on network regain,
      and after a stream drop, with bounded backoff; tests
      `client-runtime/reconnect-after-a-dropped-stream`

## 4. App

- [ ] 4.1 Wire the lifecycle (`app/lib/app/lifecycle.dart`) to the core's
      reconnect on resume and connectivity change; manual QA: background the app,
      send from another device, confirm the notification arrives within a sweep

## 5. Check

- [ ] 5.1 `cargo test --workspace`, clippy, fmt, `flutter analyze`,
      `flutter test`, `openspec validate notification-reliability --strict`;
      manual QA of the zombie-window and lost-ping cases on iOS and Android
