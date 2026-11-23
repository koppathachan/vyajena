-- Change `lobby-multi-group`: named groups (multi-group: Named group
-- creation). A named group is a `dm_groups` row of kind `named`: it has no peer
-- (empty) and keeps its last-known name, from the admin's `GroupSettings`.
-- Times are milliseconds since the Unix epoch.

ALTER TABLE dm_groups ADD COLUMN kind TEXT NOT NULL DEFAULT 'dm'
    CHECK (kind IN ('dm', 'named'));
ALTER TABLE dm_groups ADD COLUMN name TEXT;

-- At most one open DM per peer (named groups have no peer).
DROP INDEX one_open_dm_per_peer;
CREATE UNIQUE INDEX one_open_dm_per_peer ON dm_groups (peer)
    WHERE closed_at IS NULL AND kind = 'dm';
