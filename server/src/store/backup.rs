//! server-deployment: Backup to the Mac. A consistent snapshot of the live
//! database, taken with SQLite's online backup while the server keeps running
//! (server-deployment design D6), so the server needs no `sqlite3` tool.

use std::fs::OpenOptions;
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::time::Duration;

use rusqlite::backup::{Backup, StepResult};
use rusqlite::{Connection, OpenFlags};

use super::{DATABASE_FILE_MODE, StoreError};

/// How often a busy or locked step is retried, and how long apart.
const BUSY_RETRIES: u32 = 50;
const BUSY_WAIT: Duration = Duration::from_millis(100);

/// What a snapshot holds.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Snapshot {
    /// The snapshot file.
    pub path: PathBuf,
    /// Its schema version (`PRAGMA user_version`).
    pub schema_version: i64,
    /// Its size in bytes.
    pub bytes: u64,
}

/// Copies the database at `database` into a new file `to` (mode 0600), in one
/// read transaction so the copy is consistent while the server writes. Refuses
/// to overwrite `to` and checks the copy's integrity.
pub fn snapshot(database: &Path, to: &Path) -> Result<Snapshot, StoreError> {
    let io = |path: &Path| {
        let path = path.to_path_buf();
        move |source| StoreError::Io { path, source }
    };
    if !database.is_file() {
        return Err(io(database)(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "no database here",
        )));
    }
    // The snapshot is private before a byte is written, and never replaces a
    // file that is already there.
    let file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(DATABASE_FILE_MODE)
        .open(to)
        .map_err(io(to))?;
    file.set_permissions(std::fs::Permissions::from_mode(DATABASE_FILE_MODE))
        .map_err(io(to))?;
    drop(file);

    let source = Connection::open_with_flags(
        database,
        OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;
    source.busy_timeout(Duration::from_secs(5))?;
    let mut target = Connection::open(to)?;
    {
        let backup = Backup::new(&source, &mut target)?;
        // -1: every page in one step, inside one read transaction, so the
        // copy is one consistent state even while the server commits.
        let mut tries = 0;
        loop {
            match backup.step(-1)? {
                StepResult::Done => break,
                StepResult::Busy | StepResult::Locked if tries < BUSY_RETRIES => {
                    tries += 1;
                    std::thread::sleep(BUSY_WAIT);
                }
                other => {
                    return Err(StoreError::Inconsistent(format!(
                        "the online backup did not finish: {other:?}"
                    )));
                }
            }
        }
    }
    let check: String = target.query_row("PRAGMA integrity_check", [], |row| row.get(0))?;
    if check != "ok" {
        return Err(StoreError::Inconsistent(format!(
            "the snapshot failed its integrity check: {check}"
        )));
    }
    let schema_version: i64 = target.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    // A self-contained file: no -wal next to it.
    target.query_row("PRAGMA journal_mode = DELETE", [], |row| {
        row.get::<_, String>(0)
    })?;
    target.close().map_err(|(_, error)| error)?;
    let bytes = std::fs::metadata(to).map_err(io(to))?.len();
    Ok(Snapshot {
        path: to.to_path_buf(),
        schema_version,
        bytes,
    })
}
