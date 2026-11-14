-- qa-fixes D5: devices the Pi's inactivity Removes took out of the group, so
-- that their re-add (the same device id) is marked automatic and the UI shows
-- no notice for either. A row goes when the device is added again. Times are
-- milliseconds since the Unix epoch.
CREATE TABLE inactive_devices (
    device_id  BLOB PRIMARY KEY CHECK (length(device_id) = 16),
    removed_at INTEGER NOT NULL
) STRICT;
