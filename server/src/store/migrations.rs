//! Schema migrations, embedded in the binary and applied in order at start.
//! `PRAGMA user_version` records how many have been applied. Later tasks add
//! files; an applied migration is never edited.

use rusqlite::Connection;

use super::StoreError;

/// The migrations, in order. Migration `n` (1-based) moves the schema from
/// version `n - 1` to `n`.
const MIGRATIONS: &[&str] = &[
    include_str!("migrations/0001_runtime.sql"),
    include_str!("migrations/0002_devices_keys_group.sql"),
    include_str!("migrations/0003_messaging.sql"),
    include_str!("migrations/0004_join_notices.sql"),
    include_str!("migrations/0005_groups.sql"),
    include_str!("migrations/0006_blobs.sql"),
    include_str!("migrations/0007_named_groups.sql"),
    include_str!("migrations/0008_removed_conversations.sql"),
    include_str!("migrations/0009_catchup.sql"),
    include_str!("migrations/0010_mute_prefs.sql"),
    include_str!("migrations/0011_presence.sql"),
    include_str!("migrations/0012_reusable_invites.sql"),
    include_str!("migrations/0013_silent_envelopes.sql"),
];

/// The schema version this binary writes.
pub const SCHEMA_VERSION: i64 = MIGRATIONS.len() as i64;

pub(super) fn apply(conn: &mut Connection) -> Result<(), StoreError> {
    let current: i64 = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if current > SCHEMA_VERSION {
        return Err(StoreError::NewerSchema {
            found: current,
            supported: SCHEMA_VERSION,
        });
    }
    let start = usize::try_from(current).unwrap_or(0);
    if start >= MIGRATIONS.len() {
        return Ok(());
    }
    // A migration may rebuild a table other tables point at (the SQLite way to
    // change a CHECK): foreign keys are off while they run, checked before each
    // commit, and back on after.
    conn.execute_batch("PRAGMA foreign_keys = OFF")?;
    let result = run(conn, start);
    conn.execute_batch("PRAGMA foreign_keys = ON")?;
    result
}

fn run(conn: &mut Connection, start: usize) -> Result<(), StoreError> {
    for (index, sql) in MIGRATIONS.iter().enumerate().skip(start) {
        let version = index + 1;
        let tx = conn.transaction()?;
        tx.execute_batch(sql)?;
        let broken: Option<String> = tx
            .query_row("PRAGMA foreign_key_check", [], |row| row.get(0))
            .ok();
        if let Some(table) = broken {
            return Err(StoreError::Inconsistent(format!(
                "migration {version} left a broken foreign key in {table}"
            )));
        }
        tx.pragma_update(None, "user_version", version)?;
        tx.commit()?;
        tracing::info!(version, "applied database migration");
    }
    Ok(())
}
