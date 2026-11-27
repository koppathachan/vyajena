## ADDED Requirements

### Requirement: Links in messages
The app SHALL draw `http://` and `https://` URLs in message text and in image
captions as tappable links, and SHALL NOT linkify anything else (no bare domains,
`mailto:` or `tel:`). Tapping a link SHALL open a confirm sheet showing the full
destination URL with Open, Copy and Cancel, and only Open SHALL hand the URL to
the OS browser (`url_launcher`, external). The app SHALL fetch no link preview and
SHALL open no connection itself. Where a detected link range overlaps a mention,
the mention SHALL be drawn and the link range dropped. Links SHALL behave the same
in the lobby, named groups and DMs. The app enforces this.

#### Scenario: Tap a link
- **WHEN** Alice's message says "see https://example.com/x" and Bob taps the URL
- **THEN** a sheet shows "https://example.com/x" with Open, Copy and Cancel, and
  Open launches the external browser

#### Scenario: Only http/https
- **WHEN** a message says "visit example.com or mail a@b.com"
- **THEN** neither "example.com" nor "a@b.com" is tappable

#### Scenario: Link in a caption
- **WHEN** an image is captioned "https://example.com/photo"
- **THEN** the caption shows the URL as a tappable link under the image

#### Scenario: Link overlapping a mention
- **WHEN** a message's text has a mention span over the same characters as a
  detected URL
- **THEN** the app draws the mention and does not draw a second link over it

#### Scenario: Nothing is fetched
- **WHEN** a message contains a URL
- **THEN** the app shows no title, image or other preview for it and opens no
  connection until the user taps Open
