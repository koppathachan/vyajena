-- group-membership: Automatic add of new users (flutter-app task 4.4). A
-- device that was ever a group member is never announced with a JoinNotice;
-- one that left (inactivity, revocation) follows its own path.
ALTER TABLE devices ADD COLUMN was_member INTEGER NOT NULL DEFAULT 0
    CHECK (was_member IN (0, 1));

UPDATE devices SET was_member = 1
WHERE device_id IN (SELECT device_id FROM mls_members)
   OR device_id IN (SELECT device_id FROM inactive_devices);
