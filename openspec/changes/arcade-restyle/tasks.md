## 1. Foundation

- [x] 1.1 Bundle Press Start 2P and its OFL licence (`app/assets/fonts/`),
  declare the font in `pubspec.yaml`. No new dependency.
- [x] 1.2 `lib/app/arcade/`: tokens, `Steps` curve, `ArcadeBorder`, motion
  helpers; rewrite `theme.dart` (ColorScheme, TextTheme, component themes).
  Tests: `app-ui/arcade-look`, `app-ui/arcade-motion` (incl. AA contrast).
- [x] 1.3 Cap text scale at 1.3 in `MaterialApp.builder`.

## 2. Screens

- [x] 2.1 Chat bubbles, media bubbles, composer and chat tiles.
- [x] 2.2 Shell, chat list, avatars, connection indicator.
- [x] 2.3 Onboarding, admin, devices, profile screens; dialogs and sheets.
- [x] 2.4 Update widget tests that match relabelled text.

## 3. Added after owner review

- [x] 3.1 Entrance and float motion; reduce-motion switch (`app-ui/arcade-motion`).
- [x] 3.2 Appearance choice Auto/Light/Dark, kept with `shared_preferences`
  (owner approved the one dependency); tests `app-ui/appearance-choice`.
- [x] 3.3 Downloaded videos show their first frame; tests
  `app-ui/downloaded-video-shows-first-frame`.
- [x] 3.4 Lobby row styling from `lobby-distinct` in the Arcade look.
- [x] 3.5 New launcher icons (`app/store/icons.sh`).

- [x] 3.6 Dialogs, bottom sheets, pop-up menus and the join screen reviewed on a
  simulator in light and dark: dialogs scroll (no overflow under the keyboard),
  menus use the body font, text buttons use the monospace body.

## 4. Owner review 2 (design D11-D14)

- [x] 4.2 Duller dark-mode yellow `#E6D84D`; navy send icon on the yellow
  button; light-mode heading drawn twice for weight (`app-ui/arcade-look`).
- [x] 4.3 Hand-drawn lines: `sketch.dart`, `ArcadeBorder`, `SketchInputBorder`,
  `SketchEdgeBorder`, `SketchDivider` across the theme, avatars, tiles, pills,
  bars and dividers (`app-ui/hand-drawn-lines`).
- [x] 4.4 Irregular boxes: bubbles offset and turned per message, icons askew,
  send and mic buttons smaller than the field (`app-ui/irregular-boxes`).
- [x] 4.5 Groups distinct from DMs: stacked group avatar and group mark
  (`app-ui/groups-distinct-from-dms`).
- [x] 4.6 Contact tap never does nothing (`app-ui/contact-tap`).
- [x] 4.7 Sprite float on chat list and Contacts icons (`app-ui/arcade-motion`).
- [x] 4.8 Theme choice Arcade/Calm on the Profile tab, look and colours
  together, kept on the device (`app-ui/theme-choice`, design D15).

## 5. Review

- [ ] 5.1 Owner views light and dark on a simulator before commit.
