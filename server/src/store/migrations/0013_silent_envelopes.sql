-- push-silent-app-envelopes: Only a user-visible message alerts an offline
-- device. `silent` = 1 marks an APPLICATION envelope whose encrypted Content
-- carries no user-visible message (a profile update, a group-settings change or
-- a message-deletion tombstone); the sender sets vyajena.v1.Envelope.silent.
-- The wake-push gate and the catch-up sweep skip a device whose only undelivered
-- backlog is silent (and/or control) envelopes. Existing rows default to 0
-- (notifiable), the pre-fix behaviour; control rows are excluded by `kind`
-- regardless. Queue cap and application TTL do not read this column.
ALTER TABLE envelopes ADD COLUMN silent INTEGER NOT NULL DEFAULT 0
    CHECK (silent IN (0, 1));
