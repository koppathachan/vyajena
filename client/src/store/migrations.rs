//! Schema migrations, embedded in the library and applied in order on open.
//! `PRAGMA user_version` records how many have been applied. Later task groups
//! add files; an applied migration is never edited.

use rusqlite::Connection;

use super::StoreError;

/// The migrations, in order. Migration `n` (1-based) moves the schema from
/// version `n - 1` to `n`.
const MIGRATIONS: &[&str] = &[
    include_str!("migrations/0001_local_store.sql"),
    include_str!("migrations/0002_device_state.sql"),
    include_str!("migrations/0003_messaging.sql"),
    include_str!("migrations/0004_inactive_devices.sql"),
    include_str!("migrations/0005_mentions.sql"),
    include_str!("migrations/0006_dms.sql"),
    include_str!("migrations/0007_media.sql"),
    include_str!("migrations/0008_blob_pending.sql"),
    include_str!("migrations/0009_named_groups.sql"),
    include_str!("migrations/0010_identity_retrust.sql"),
    include_str!("migrations/0011_deleted_messages.sql"),
    include_str!("migrations/0012_mute_prefs.sql"),
];

/// The schema version this library writes.
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
