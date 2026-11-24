## ADDED Requirements

### Requirement: Delete a blob
A device SHALL purge a stored blob with `Blobs.DeleteBlob`, naming the `group_id` and
`blob_id`. The server SHALL authorise it with the same authority as the message
delete (message-deletion: Who may delete): an admin device of the `group_id` in the
lobby or a named group, or the device that uploaded the blob in a DM. The server
SHALL record the uploading device per blob so a DM purge can be authorised, and SHALL
reject an unauthorised call with `PERMISSION_DENIED`, purging nothing. The server
SHALL hold one copy per `blob_id`, SHALL delete it, and SHALL answer `NOT_FOUND` for
it thereafter. A `DeleteBlob` for an expired, already-deleted or unknown blob SHALL
succeed as a no-op. The server enforces this.

#### Scenario: Admin purges a group blob
- **WHEN** the admin deletes a media message in a named group and the client calls
  `Blobs.DeleteBlob` for its `blob_id`
- **THEN** the server removes the blob, and a later `Blobs.DownloadBlob` answers
  `NOT_FOUND`

#### Scenario: Uploader purges a DM blob
- **WHEN** Alice deletes a media message she sent in her DM with Bob and her client
  calls `Blobs.DeleteBlob`
- **THEN** the server removes the one stored copy

#### Scenario: Unauthorised purge refused
- **WHEN** a non-admin device calls `Blobs.DeleteBlob` for a blob in a named group
- **THEN** the server rejects it with `PERMISSION_DENIED` and keeps the blob

#### Scenario: Purge of an absent blob
- **WHEN** `Blobs.DeleteBlob` names a blob that has already expired
- **THEN** the call succeeds and removes nothing
