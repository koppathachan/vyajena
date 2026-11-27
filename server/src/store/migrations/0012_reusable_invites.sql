-- Change `review-admin-invite`: the store reviewers' server keeps one admin
-- invite that never expires and is never used up (device-identity: Reusable
-- review admin invite). Only a server whose config sets
-- `[review] reusable_admin_invite = true` creates or honours such a row; every
-- other invite stays single-use and expiring.
ALTER TABLE invites ADD COLUMN reusable INTEGER NOT NULL DEFAULT 0
    CHECK (reusable IN (0, 1));
