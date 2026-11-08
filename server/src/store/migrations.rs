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
    for (index, sql) in MIGRATIONS.iter().enumerate().skip(start) {
        let version = index + 1;
        let tx = conn.transaction()?;
        tx.execute_batch(sql)?;
        tx.pragma_update(None, "user_version", version)?;
        tx.commit()?;
        tracing::info!(version, "applied database migration");
    }
    Ok(())
}
