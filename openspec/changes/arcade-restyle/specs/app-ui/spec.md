## REMOVED Requirements

### Requirement: Light Material 3 look
**Reason**: Replaced by the Arcade look; the app no longer uses `ColorScheme.fromSeed`,
stock widgets or the system font.
**Migration**: See `Arcade look` and `Arcade motion` below.

## ADDED Requirements

### Requirement: Arcade look
The app SHALL use Material 3 widgets themed from one explicit token set, light
and dark following the system. Every shape SHALL have square (unrounded)
corners, drawn by hand as in `Hand-drawn lines`. Borders SHALL be
2px (white in dark, navy `#0F0F23` in light). Depth SHALL be a hard offset shadow
with no blur: 6px on cards, 3px on buttons, 2px on chat bubbles. Headings and
labels SHALL use the bundled Press Start 2P font and body text SHALL use a
monospace font, never the pixel font. Text and background colours SHALL meet
WCAG AA contrast. The only package added for this is `shared_preferences`, which
keeps the appearance choice.

#### Scenario: Dark mode
- **WHEN** Alice's phone is in dark mode
- **THEN** the app uses the dark tokens: background `#0F0F23`, text white, cards
  `#1B1B3A`, white 2px borders and yellow `#E6D84D` buttons (a duller yellow
  than `#FFFF00`, easier on the eyes), with navy icons and text on them,
  the send icon included

#### Scenario: Light mode
- **WHEN** Alice's phone is in light mode
- **THEN** the app uses the light tokens: background `#FFF8E7`, navy text and
  borders, white cards and purple `#764BA2` buttons, and the hard shadows are
  visible; headings are drawn twice 0.6px apart, so an app-bar title such as
  "Chats" looks as thick as in dark mode

#### Scenario: Dialogs, sheets and menus
- **WHEN** Alice opens a dialog, a bottom sheet, a pop-up menu or a snack bar
- **THEN** it is square with the 2px border, its dialog title uses the pixel
  font, its text the monospace body font, and a dialog that holds a text field
  scrolls rather than overflowing under the keyboard

#### Scenario: Body is never pixel
- **WHEN** a chat message is shown
- **THEN** its text uses the monospace body font

### Requirement: Arcade motion
Motion SHALL use stepped curves only. Hover and press SHALL take 200ms in 4
steps; an entrance slide 800ms in 8 steps with a 120ms stagger; a float one
3s loop of at most 8 frames, each a whole number of pixels high (a sprite's
jump, never a glide). The Profile avatar, the chat list's row icons and the
Contacts tab's icons float, each from its own point in the loop. The message list SHALL have no entrance or hover motion. Text scale SHALL
be capped at 1.3. When the system asks to reduce motion, the app SHALL run no
animation or transition of its own.

#### Scenario: Chat list and screens slide in once
- **WHEN** the chat list or the Profile, Contacts or join screen first appears
- **THEN** its rows slide up into place in 8 steps, 120ms apart, and a row does not
  slide in again when it is scrolled back or the list reloads

#### Scenario: Icons bob like sprites
- **WHEN** the chat list or the Contacts tab is shown
- **THEN** each row's icon bobs up and down by whole pixels a few times a
  second, not in step with its neighbours

#### Scenario: Message list stays instant
- **WHEN** a message arrives in a chat
- **THEN** it appears with no entrance animation

#### Scenario: Reduced motion
- **WHEN** the phone's reduce-motion setting is on
- **THEN** no entrance, float or connection pulse runs

### Requirement: Appearance choice
The Profile tab SHALL offer Auto, Light and Dark. Auto, the default, SHALL follow
the phone's light or dark setting; Light and Dark SHALL override it, and the
choice SHALL be kept on this device between runs.

#### Scenario: Choose Dark
- **WHEN** Alice picks Dark on the Profile tab while her phone is in light mode
- **THEN** the app turns dark at once and is dark the next time it opens

#### Scenario: Auto follows the phone
- **WHEN** Alice has never chosen, and her phone switches to dark mode
- **THEN** the app is dark

### Requirement: Downloaded video shows its first frame
A video bubble SHALL show the clip's real first frame in place of the blur once
the clip is in the device's cache, so a downloaded video reads as downloaded. It
SHALL NOT download anything to do so; before the clip is cached the bubble shows
the blur as before.

#### Scenario: After the first play
- **WHEN** Alice taps a video, it downloads and plays, and she returns to the chat
- **THEN** the bubble shows the video's first frame

#### Scenario: Not downloaded
- **WHEN** a video has not been downloaded
- **THEN** its bubble shows the blur and nothing is downloaded

### Requirement: Lobby row in the Arcade look
The lobby row's styling (change `lobby-distinct`) SHALL use the Arcade look: the
`#` tile is a bordered square in the accent colours, not a rounded one.

#### Scenario: Square hash tile
- **WHEN** the chat list shows the lobby
- **THEN** its `#` tile is a square with the 2px border

### Requirement: Hand-drawn lines
Every line the app draws SHALL look drawn by hand: box outlines (cards,
buttons, fields, dialogs, sheets, menus, bubbles, avatars, tiles, pills), the
app bar's and navigation bar's edges, dividers and quote bars. A box's corners
SHALL sit up to 2px off true and its edges SHALL bow slightly, drawn as two
strokes; its fill and hard shadow SHALL follow the same outline. A box's shape
SHALL come from a seed and its size, so it is the same on every repaint.

#### Scenario: A button is not ruled
- **WHEN** Alice looks at any button or card
- **THEN** its outline is a slightly crooked, hand-drawn box, and it looks the
  same each time the screen redraws

### Requirement: Irregular boxes
Boxes SHALL NOT line up as if on a grid. In a chat each bubble SHALL sit a
few pixels off the column and be turned by under a degree, from its message
id; chat-list and Contacts icons SHALL be set a little askew; the composer's
send and microphone buttons SHALL be smaller than the text box and sit off its
line.

#### Scenario: Stacked bubbles
- **WHEN** a chat shows several messages from Alice in a row
- **THEN** their bubbles do not share an edge or a tilt, and each keeps its
  place when the list redraws

#### Scenario: The composer
- **WHEN** Alice types a message
- **THEN** the send button is smaller than the text box and not aligned with
  its edges

### Requirement: Groups distinct from DMs
In the chat list a named group's row SHALL differ from a DM's: its avatar is
a stack of two tiles and its name carries a group mark, where a DM shows one
person's single tile (with the presence dot) and a plain name. The lobby keeps
its own `#` tile and system pill.

#### Scenario: Group and DM rows
- **WHEN** the chat list shows the lobby, the "Weekend" group and a DM with Bob
- **THEN** the lobby has the `#` tile, Weekend a stacked tile and a group
  mark, and Bob's row one tile and no mark

### Requirement: Contact tap
Tapping any row on the Contacts tab SHALL do something: another member's row
opens or starts the DM with them, asking the core afresh whether DMs are
offered (the answer can arrive after the tab first loaded); when DMs are not
offered, a snack bar says so; the user's own row opens the Profile tab.

#### Scenario: DMs learnt after the tab loaded
- **WHEN** the Contacts tab loaded before the device connected, and Alice taps
  Bob once the server says it offers DMs
- **THEN** the DM with Bob opens

#### Scenario: Own row
- **WHEN** Alice taps her own row
- **THEN** the Profile tab opens
