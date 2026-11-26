-- Change `report-and-block`: this device's local block list (moderation: Block a
-- user). Purely local to the device: it is never synced to the server or to the
-- user's other devices (design D1), so it holds only the blocked `UserId`s as
-- text. The display and notification paths skip a blocked user's messages; the
-- messages themselves stay stored and reappear on unblock.
CREATE TABLE blocked_users (
    user_id TEXT NOT NULL PRIMARY KEY
) STRICT, WITHOUT ROWID;
