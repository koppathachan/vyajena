-- Change `notification-reliability` (D5): the durable catch-up attempt counter.
-- One row per offline device the sweep is still nudging with wake pings, holding
-- how many catch-up pings it has sent for the current backlog. Kept in SQLite,
-- not only in memory, so the bounded attempt budget (CATCH_UP_MAX) and the
-- "keep nudging" survive a server restart. Holds no message content and no
-- history: just a device id and a count, deleted when the device drains its
-- backlog or reconnects (message-delivery: Sweep catch-up wake ping).
CREATE TABLE catchup (
    device_id BLOB PRIMARY KEY REFERENCES devices (device_id),
    attempts  INTEGER NOT NULL
) STRICT, WITHOUT ROWID;
