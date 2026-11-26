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

**D11 Owner review 2 (2026-11-27).** After using it on a phone the owner asked
for: a duller dark-mode yellow (`#E6D84D` replaces `#FFFF00`; contrast with navy
stays far above AA); navy, not white, on the yellow send button (the icon-button
theme's white foreground had won over the filled variant, so the send buttons
take an explicit style); no box that looks laid on a grid and every line hand
drawn; groups distinct from DMs; contact taps that never do nothing; a thicker
light-mode heading; and choppy, pixel-step motion on the chat list and Contacts
icons too. This replaces "radius 0, straight 2px borders" with hand-drawn
square boxes (D12) and the 5-step float with a frame loop (D13).

**D12 Hand-drawn boxes, no package.** `sketch.dart` draws each box from its
four corners moved up to 2px, edges as cubics bowed up to 1.2px with ends that
overshoot, stroked twice (0.9 and 0.55 of the width) like a pen going over it.
`ArcadeBorder` keeps its name and fields (plus a `seed`) so every theme and test
that names it still holds; `SketchInputBorder` does the same for fields (it
leaves the floating label's gap open) and `SketchEdgeBorder` for the app bar,
navigation bar and quote bars. The wobble is seeded by role or id and mixed
with the box's rounded size: stable on repaint, different between boxes. Fill,
clip and shadow use the same outline. Irregular placement is separate:
`Askew` (rotation and shift from a seed) on list icons and the send button, and
a per-message margin and sub-degree turn on bubbles; a sticker is not turned.

**D13 Sprite float.** One 3s loop of 8 frames (0, 0, ½, 1, 1, 1, ½, 0 of the
height), snapped to whole logical pixels, about 375ms a frame, rebuilt only when
the frame changes. Each icon starts at its own phase (from its id). Still under
reduce-motion.

**D14 Contact taps.** The Contacts tab read "DMs offered?" once, often before
the device had connected, and left rows with no tap. It now listens to the
connection, asks the core again on each tap, explains when DMs are not offered,
and sends a tap on your own row to the Profile tab.

## Verification

Widget tests on the fake core; the owner reviews light and dark on a simulator
before anything is committed.
