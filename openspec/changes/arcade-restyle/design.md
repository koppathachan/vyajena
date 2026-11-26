## Decisions

**D1 Tokens, not a seed.** `ArcadeTokens` (a `ThemeExtension`) plus an explicit
`ColorScheme` per brightness hold the palette; widgets keep reading
`Theme.of(context).colorScheme`, so most of the app restyles from `theme.dart`.
Colours outside the owner's validated palette (error, muted text, mention
border, presence, disabled) are derived and checked for WCAG AA by a test.

**D2 Hard shadows with a ShapeBorder.** Material elevation is blurred, so
`ArcadeBorder` (a `ShapeBorder`) paints the 2px border and a blur-0 offset shadow
itself; cards, buttons, dialogs and sheets get it from their component themes.
Chat bubbles use the same border with the 2px offset. In dark mode a black
shadow is nearly invisible on the navy background; the white border carries the
edge (inherent to the owner's palette).

**D3 Fonts.** Press Start 2P (118 KB, OFL) is the only bundled font; it has a
Reserved Font Name, so it is not subset or renamed. Body text is
`monospace` with fallback `Menlo`/`Courier` (0 bytes). The pixel font is wide
(one em per glyph): it is used only for short headings and labels, and any label
that does not fit one line at its size falls back to the body font. The
`google_fonts` package is not used (runtime network fetch).

**D4 Motion.** `Steps(n)` is a 10-line `Curve`. Hover maps to the pressed state
on a phone. Entrance (800ms steps(8), 120ms stagger) runs only on the first paint
of onboarding, admin and profile content and the chat list, never on route
changes and never on the message list. Float (3s steps(5)) is one decorative
widget. `MediaQuery.disableAnimations` turns all of it off.

**D5 Text scale cap 1.3** as briefed. WCAG 1.4.4 asks for 200%; the owner chose
1.3, recorded here as a known trade-off.

**D6 Uppercase** is applied through one helper to app-bar titles, headings and
short labels, never to user content, chat bodies or error text.

**D7 Reduced from the brief after phone review (owner, 2026-11-27).** The brief
asked for 4px borders and 12/4/6px shadows; on a phone that was too heavy and
cluttered, so borders are 2px and shadows 6/3/2px, margins tightened, and the
app-bar title is 16px (a whole 2px per pixel of the font) with a 2px shadow.
Headlines keep the 4px red shadow.

**D8 Appearance choice.** Auto/Light/Dark on the Profile tab; Auto is
`ThemeMode.system`. The choice is one string in `shared_preferences` (the owner
approved this one dependency, 2026-11-27); a store interface lets tests keep it in
memory. **D9 First frame.** A cached clip's first frame is drawn by a paused,
silent `video_player` in the bubble (disposed with it), over the blur; a
file that cannot be opened keeps the blur. **D10 Entrance ids.** A row slides in
the first time its id is shown in a run, so reloads and scrolling do not replay it.

## Verification

Widget tests on the fake core; the owner reviews light and dark on a simulator
before anything is committed.
