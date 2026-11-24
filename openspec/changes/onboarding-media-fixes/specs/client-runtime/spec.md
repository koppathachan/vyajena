## ADDED Requirements

### Requirement: Media the server will not store fails promptly
The core SHALL stop retrying a blob upload and fail the message with a clear reason
after the server has answered `UploadBlob` with `UNAVAILABLE` for a short bounded
number of consecutive attempts, rather than retrying silently until the blob's
time-to-live expires (seven days). The failure reason SHALL name the likely cause
(the server is not storing pictures / voice messages). A transient `UNAVAILABLE`
that clears within the bound still uploads; other retryable statuses
(unauthenticated, timeout) are unaffected and keep their existing retry policy.
The core enforces this and reports the failed send status to the UI (app-ui: Media
the server will not store shows an error).

#### Scenario: Persistent UNAVAILABLE fails the upload
- **WHEN** a connected session uploads a voice message's blob and the server
  answers `UNAVAILABLE` on every attempt because it has no blob store
- **THEN** after the bounded number of consecutive `UNAVAILABLE` attempts the core
  fails the message with a reason, instead of waiting seven days

#### Scenario: A brief outage still uploads
- **WHEN** the server answers `UNAVAILABLE` once or twice (its drive is remounting)
  and then accepts the blob within the bound
- **THEN** the upload succeeds and the message sends, with no failure shown
