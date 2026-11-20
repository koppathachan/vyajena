//! The DMs this device is in, per-group bookkeeping that works for the main
//! group and DMs alike, chat read marks and one-time upgrade flags (change
//! `dms`, design D9).
//!
//! Chat 0 is the main group, whose bookkeeping stays in `group_state`; a DM
//! is a `dm_groups` row whose `chat_id` is its chat id.

use std::time::SystemTime;

use rusqlite::{OptionalExtension, params};

use super::{GroupRow, Store, StoreError};
use crate::clock::{from_unix_millis, unix_millis};

/// The chat id of the main group.
pub const MAIN_CHAT: i64 = 0;

/// One DM this device opened, joined or waits to join.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DmRow {
    /// Its chat id (never [`MAIN_CHAT`]).
    pub chat: i64,
    /// The MLS group id.
    pub group_id: Vec<u8>,
    /// `<username>@<server_id>` of the other user.
    pub peer: String,
    /// When this device first knew of it.
    pub created_at: SystemTime,
    /// Whether this device has the DM's MLS state.
    pub joined: bool,
    /// The group bookkeeping, as for the main group.
    pub state: GroupRow,
    /// When the server closed it (direct-messages: DM closed on user removal).
    pub closed_at: Option<SystemTime>,
    /// Whether this is a named group rather than a DM (multi-group); then
    /// `peer` is empty.
    pub named: bool,
    /// A named group's last-known name (message-content: Group settings travel
    /// encrypted).
    pub name: Option<String>,
}

fn seq_sql(seq: u64) -> i64 {
    i64::try_from(seq).unwrap_or(i64::MAX)
}

const DM_COLUMNS: &str = "chat_id, group_id, peer, created_at, joined, own_leaf_updated_at, \
                          processed_seq, stalled, closed_at, kind, name";

type RawDm = (
    i64,
    Vec<u8>,
    String,
    i64,
    bool,
    i64,
    i64,
    Option<String>,
    Option<i64>,
    String,
    Option<String>,
);

fn raw_dm(row: &rusqlite::Row<'_>) -> rusqlite::Result<RawDm> {
    Ok((
        row.get(0)?,
        row.get(1)?,
        row.get(2)?,
        row.get(3)?,
        row.get(4)?,
        row.get(5)?,
        row.get(6)?,
        row.get(7)?,
        row.get(8)?,
        row.get(9)?,
        row.get(10)?,
    ))
}

fn dm(
    (chat, group_id, peer, created_at, joined, leaf, processed, stalled, closed, kind, name): RawDm,
) -> DmRow {
    DmRow {
        chat,
        group_id: group_id.clone(),
        peer,
        created_at: from_unix_millis(created_at),
        joined,
        state: GroupRow {
            group_id,
            own_leaf_updated_at: from_unix_millis(leaf),
            processed_seq: u64::try_from(processed).unwrap_or(0),
            stalled,
        },
        closed_at: closed.map(from_unix_millis),
        named: kind == "named",
        name,
    }
}

impl Store {
    fn dm_where(
        &self,
        clause: &str,
        params: impl rusqlite::Params,
    ) -> Result<Vec<DmRow>, StoreError> {
        let rows = self.db.with(|conn| {
            let mut statement =
                conn.prepare(&format!("SELECT {DM_COLUMNS} FROM dm_groups {clause}"))?;
            let rows = statement.query_map(params, raw_dm)?;
            rows.collect::<rusqlite::Result<Vec<RawDm>>>()
        })?;
        Ok(rows.into_iter().map(dm).collect())
    }

    /// Every DM, open ones first, oldest first.
    pub fn dms(&self) -> Result<Vec<DmRow>, StoreError> {
        self.dm_where("ORDER BY closed_at IS NOT NULL, chat_id", [])
    }

    /// The DM with MLS group id `group_id`.
    pub fn dm_by_group(&self, group_id: &[u8]) -> Result<Option<DmRow>, StoreError> {
        Ok(self
            .dm_where("WHERE group_id = ?1", params![group_id])?
            .pop())
    }

    /// The DM with chat id `chat`.
    pub fn dm_by_chat(&self, chat: i64) -> Result<Option<DmRow>, StoreError> {
        Ok(self.dm_where("WHERE chat_id = ?1", params![chat])?.pop())
    }

    /// The open DM with `peer` (`<username>@<server_id>`), if any.
    pub fn open_dm_with(&self, peer: &str) -> Result<Option<DmRow>, StoreError> {
        Ok(self
            .dm_where(
                "WHERE peer = ?1 AND closed_at IS NULL AND kind = 'dm'",
                params![peer],
            )?
            .pop())
    }

    /// Records a DM this device has MLS state for at `now` (it created it, or
    /// joined from a Welcome having processed up to `processed_seq`), with a
    /// fresh leaf and no stall. Returns its chat id.
    pub fn set_dm_joined(
        &self,
        group_id: &[u8],
        peer: &str,
        now: SystemTime,
        processed_seq: u64,
    ) -> Result<i64, StoreError> {
        self.db.with(|conn| {
            conn.execute(
                "INSERT INTO dm_groups (group_id, peer, created_at, joined, own_leaf_updated_at,
                                        processed_seq)
                 VALUES (?1, ?2, ?3, 1, ?3, ?4)
                 ON CONFLICT (group_id) DO UPDATE SET joined = 1, own_leaf_updated_at = ?3,
                     processed_seq = max(processed_seq, ?4), stalled = NULL",
                params![group_id, peer, unix_millis(now), seq_sql(processed_seq)],
            )
        })?;
        self.dm_by_group(group_id)?
            .map(|row| row.chat)
            .ok_or_else(|| StoreError::Inconsistent("a DM row vanished".into()))
    }

    /// Records a DM this device waits to join (both users opened it at once:
    /// the server kept the other's). Returns its chat id.
    pub fn set_dm_waiting(
        &self,
        group_id: &[u8],
        peer: &str,
        now: SystemTime,
    ) -> Result<i64, StoreError> {
        self.db.with(|conn| {
            conn.execute(
                "INSERT INTO dm_groups (group_id, peer, created_at, joined, own_leaf_updated_at)
                 VALUES (?1, ?2, ?3, 0, ?3)
                 ON CONFLICT (group_id) DO NOTHING",
                params![group_id, peer, unix_millis(now)],
            )
        })?;
        self.dm_by_group(group_id)?
            .map(|row| row.chat)
            .ok_or_else(|| StoreError::Inconsistent("a DM row vanished".into()))
    }

    /// Records a named group this device has MLS state for (it created it, or
    /// joined from a Welcome having processed up to `processed_seq`). Returns
    /// its chat id.
    pub fn set_named_joined(
        &self,
        group_id: &[u8],
        now: SystemTime,
        processed_seq: u64,
    ) -> Result<i64, StoreError> {
        self.db.with(|conn| {
            conn.execute(
                "INSERT INTO dm_groups (group_id, peer, created_at, joined, own_leaf_updated_at,
                                        processed_seq, kind)
                 VALUES (?1, '', ?2, 1, ?2, ?3, 'named')
                 ON CONFLICT (group_id) DO UPDATE SET joined = 1, own_leaf_updated_at = ?2,
                     processed_seq = max(processed_seq, ?3), stalled = NULL",
                params![group_id, unix_millis(now), seq_sql(processed_seq)],
            )
        })?;
        self.dm_by_group(group_id)?
            .map(|row| row.chat)
            .ok_or_else(|| StoreError::Inconsistent("a named group row vanished".into()))
    }

    /// Sets a named group's name (from the admin's `GroupSettings`). Returns
    /// whether it changed.
    pub fn set_named_name(&self, group_id: &[u8], name: &str) -> Result<bool, StoreError> {
        let changed = self.db.with(|conn| {
            conn.execute(
                "UPDATE dm_groups SET name = ?2
                 WHERE group_id = ?1 AND kind = 'named' AND coalesce(name, '') <> ?2",
                params![group_id, name],
            )
        })?;
        Ok(changed > 0)
    }

    /// The server closed the DM at `at`.
    pub fn close_dm(&self, group_id: &[u8], at: SystemTime) -> Result<(), StoreError> {
        self.db.with(|conn| {
            conn.execute(
                "UPDATE dm_groups SET closed_at = coalesce(closed_at, ?2), joined = 0
                 WHERE group_id = ?1",
                params![group_id, unix_millis(at)],
            )
        })?;
        Ok(())
    }

    /// The bookkeeping of `group_id`: the main group's or a joined DM's.
    pub fn group_for(&self, group_id: &[u8]) -> Result<Option<GroupRow>, StoreError> {
        if let Some(main) = self.group()?
            && main.group_id == group_id
        {
            return Ok(Some(main));
        }
        Ok(self
            .dm_by_group(group_id)?
            .filter(|dm| dm.joined && dm.closed_at.is_none())
            .map(|dm| dm.state))
    }

    /// Whether `group_id` is the main group's.
    pub fn is_main(&self, group_id: &[u8]) -> Result<bool, StoreError> {
        Ok(self.group()?.is_some_and(|main| main.group_id == group_id))
    }

    /// Everything of `group_id` up to `seq` is durably processed.
    pub fn set_processed_seq_in(&self, group_id: &[u8], seq: u64) -> Result<(), StoreError> {
        if self.is_main(group_id)? {
            return self.set_processed_seq(seq);
        }
        self.db.with(|conn| {
            conn.execute(
                "UPDATE dm_groups SET processed_seq = max(processed_seq, ?2) WHERE group_id = ?1",
                params![group_id, seq_sql(seq)],
            )
        })?;
        Ok(())
    }

    /// This device stopped following `group_id`, because of `why`.
    pub fn set_stalled_in(&self, group_id: &[u8], why: &str) -> Result<(), StoreError> {
        if self.is_main(group_id)? {
            return self.set_stalled(why);
        }
        self.db.with(|conn| {
            conn.execute(
                "UPDATE dm_groups SET stalled = ?2 WHERE group_id = ?1",
                params![group_id, why],
            )
        })?;
        Ok(())
    }

    /// This device's leaf in `group_id` was replaced at `now`.
    pub fn set_own_leaf_updated_in(
        &self,
        group_id: &[u8],
        now: SystemTime,
    ) -> Result<(), StoreError> {
        if self.is_main(group_id)? {
            return self.set_own_leaf_updated(now);
        }
        self.db.with(|conn| {
            conn.execute(
                "UPDATE dm_groups SET own_leaf_updated_at = ?2 WHERE group_id = ?1",
                params![group_id, unix_millis(now)],
            )
        })?;
        Ok(())
    }

    /// The chat id of `group_id`: [`MAIN_CHAT`] for the main group (or an
    /// unknown group), else its DM's.
    pub fn chat_of(&self, group_id: &[u8]) -> Result<i64, StoreError> {
        Ok(self.dm_by_group(group_id)?.map_or(MAIN_CHAT, |dm| dm.chat))
    }

    /// The user has seen chat `chat` up to message `up_to` (a local id).
    pub fn mark_chat_read(&self, chat: i64, up_to: i64) -> Result<(), StoreError> {
        self.db.with(|conn| {
            conn.execute(
                "INSERT INTO chat_reads (chat_id, read_up_to) VALUES (?1, ?2)
                 ON CONFLICT (chat_id) DO UPDATE SET read_up_to = max(read_up_to, ?2)",
                params![chat, up_to],
            )
        })?;
        Ok(())
    }

    /// The read mark of `chat`: the local id of the newest message the user
    /// had read (as [`Store::mark_chat_read`] last set it), or 0 when the
    /// chat was never read. Only reads; never advances the mark.
    pub fn chat_read_mark(&self, chat: i64) -> Result<i64, StoreError> {
        let up_to: Option<i64> = self.db.with(|conn| {
            conn.query_row(
                "SELECT read_up_to FROM chat_reads WHERE chat_id = ?1",
                params![chat],
                |row| row.get(0),
            )
            .optional()
        })?;
        Ok(up_to.unwrap_or(0))
    }

    /// Received chat messages in `chat` after its read mark.
    pub fn unread_in(&self, chat: i64) -> Result<u32, StoreError> {
        let count: i64 = self.db.with(|conn| {
            conn.query_row(
                "SELECT count(*) FROM messages
                 WHERE chat_id = ?1 AND kind = 'chat' AND outgoing = 0
                   AND local_id > coalesce(
                       (SELECT read_up_to FROM chat_reads WHERE chat_id = ?1), 0)",
                params![chat],
                |row| row.get(0),
            )
        })?;
        Ok(u32::try_from(count).unwrap_or(u32::MAX))
    }

    /// When the newest chat message in `chat` was stored, if any.
    pub fn last_message_at(&self, chat: i64) -> Result<Option<SystemTime>, StoreError> {
        let at: Option<i64> = self.db.with(|conn| {
            conn.query_row(
                "SELECT max(created_at) FROM messages WHERE chat_id = ?1 AND kind = 'chat'",
                params![chat],
                |row| row.get(0),
            )
        })?;
        Ok(at.map(from_unix_millis))
    }

    /// Whether the one-time step `name` was done.
    pub fn flag(&self, name: &str) -> Result<bool, StoreError> {
        Ok(self
            .db
            .with(|conn| {
                conn.query_row(
                    "SELECT 1 FROM core_flags WHERE name = ?1",
                    params![name],
                    |_| Ok(()),
                )
                .optional()
            })?
            .is_some())
    }

    /// The one-time step `name` was done at `now`.
    pub fn set_flag(&self, name: &str, now: SystemTime) -> Result<(), StoreError> {
        self.db.with(|conn| {
            conn.execute(
                "INSERT INTO core_flags (name, set_at) VALUES (?1, ?2) ON CONFLICT DO NOTHING",
                params![name, unix_millis(now)],
            )
        })?;
        Ok(())
    }
}
