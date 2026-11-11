# vyajena2 app

The Flutter app (iOS and Android only). It draws screens and calls the Rust
core through `flutter_rust_bridge`; see `openspec/changes/flutter-app/` and
`CLAUDE.md` (layout, the codegen command).

- `lib/src/rust/`: generated Dart bindings of `bridge/` (committed; regenerate
  with `flutter_rust_bridge_codegen generate` in this folder, never by hand).
- `rust_builder/`: the cargokit plugin that builds `bridge/` from Xcode and
  Gradle.
