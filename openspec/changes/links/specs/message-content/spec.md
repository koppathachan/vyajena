## MODIFIED Requirements

### Requirement: Text only
`Text` SHALL hold UTF-8 text and nothing else: no attachments, link previews or
rich formatting. The client SHALL limit text to what encrypts within the 65,536
byte ciphertext cap. A client MAY draw a URL in the text as a tappable link that
opens externally (app-ui: Links in messages); this is display-only and SHALL NOT
fetch, store or send a preview, title or image for the URL, so the "no link
previews" rule still holds.

#### Scenario: Very long paste
- **WHEN** a user pastes text that would encrypt to more than 65,536 bytes
- **THEN** the app refuses to send it and tells the user it is too long

#### Scenario: A URL is still just text
- **WHEN** a message contains "https://example.com"
- **THEN** the encrypted `Content` carries only the `Text` with that string, and no
  preview is fetched or stored
