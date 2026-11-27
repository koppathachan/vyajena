## 0. Before starting

- [x] 0.1 No proto or core change; this is an `app/`-only change

## 1. App

- [x] 1.1 A URL detector (`http`/`https` only, trailing punctuation trimmed, UTF-16
      ranges); unit tests for the matcher (schemes, trailing `.`, no bare domains)
- [x] 1.2 Fold link ranges into the span builder beside mentions, dropping a link
      range that overlaps a mention; text and captions both linkified; widget
      tests `app-ui/links-in-messages`
- [x] 1.3 Confirm sheet (full URL, Open / Copy / Cancel) and `url_launcher`
      external open; "couldn't open" fallback; widget tests
      `app-ui/links-in-messages`
- [x] 1.4 `url_launcher` added to `app/pubspec.yaml`

## 2. Check

- [x] 2.1 `cd app && flutter analyze`, `flutter test`,
      `openspec validate links --strict`
