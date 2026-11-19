-- media-blobs task 4.2-4.3: the device media cache's index and the images known
-- to have expired. The bytes themselves are files in the media directory,
-- outside this database (client-runtime: Image download and cache). Times are
-- Unix milliseconds.

-- One row per cached image, keyed by blob id. `own` is set for images this
-- device sent, kept for the blob's 7 days so "resend" works
-- (client-runtime: Expired media).
CREATE TABLE media_cache (
    blob_id    BLOB PRIMARY KEY CHECK (length(blob_id) = 16),
    size       INTEGER NOT NULL CHECK (size > 0),
    own        INTEGER NOT NULL CHECK (own IN (0, 1)),
    created_at INTEGER NOT NULL,
    last_used  INTEGER NOT NULL
) STRICT;

CREATE INDEX media_cache_by_use ON media_cache (last_used);

-- Blobs the Pi answered NOT_FOUND for: the image message shows "media
-- expired" instead of offering a download.
CREATE TABLE media_expired (
    blob_id   BLOB PRIMARY KEY CHECK (length(blob_id) = 16),
    marked_at INTEGER NOT NULL
) STRICT;
