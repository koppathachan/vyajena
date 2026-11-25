## ADDED Requirements

### Requirement: Create a sticker
The app SHALL offer a make-a-sticker flow wherever it offers images (the lobby, named
groups and DMs): pick a photo, run the on-device native cutout through a
`SegmentationSource`, show a preview of the transparent result over a checkerboard so
the cutout is visible, and on send hand the RGBA cutout to the core, which encodes
and sends it as an image. The app SHALL NOT encode, encrypt or upload the sticker
itself — it goes through the core, like any image. When segmentation finds no subject
or the platform API is unavailable, the app SHALL tell the user and send nothing. A
received transparent image SHALL be rendered without an opaque bubble fill so its
transparency reads. The app enforces going through the core.

#### Scenario: Pick, preview, send
- **WHEN** Alice picks a photo, sees the cutout preview and taps send in a DM
- **THEN** the app hands the RGBA cutout to the core, shows the sticker as sending
  until accepted, and never encodes or uploads it itself

#### Scenario: No subject to cut out
- **WHEN** Alice picks a photo with no clear subject
- **THEN** the app tells her no subject was found and sends nothing

#### Scenario: Sticker shows transparent
- **WHEN** Bob receives a sticker
- **THEN** the image bubble renders it without an opaque fill so the transparent
  background reads
