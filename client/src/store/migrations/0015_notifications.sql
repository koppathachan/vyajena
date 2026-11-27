-- Change `smarter-notifications`: what the device needs to build a local
-- notification (local-notifications: The device builds the notification locally).
--
-- Admin-events that change this user's own access (removed from a named group,
-- a conversation closed, added to a new named group), kept until the app has
-- notified or the user is looking at the app. A group id and a kind only: no
-- text, no sender (Admin-event notifications are fixed, content-free phrases).
CREATE TABLE notification_admin_events (
    id INTEGER NOT NULL PRIMARY KEY AUTOINCREMENT,
    kind TEXT NOT NULL CHECK (kind IN ('removed', 'closed', 'added')),
    group_id BLOB NOT NULL,
    at INTEGER NOT NULL,
    UNIQUE (kind, group_id)
) STRICT;

-- The per-device "Notification detail" level (app-ui: Notification detail
-- level): the single row (id = 1) exists once chosen; no row means Basic.
CREATE TABLE notification_detail (
    id INTEGER NOT NULL PRIMARY KEY CHECK (id = 1),
    level TEXT NOT NULL CHECK (level IN ('basic', 'more'))
) STRICT;
