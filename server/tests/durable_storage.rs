//! pi-runtime: Durable storage.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

use rusqlite::params;
use server::store::{SCHEMA_VERSION, Store, StoreError};

fn mode(path: &Path) -> u32 {
    fs::metadata(path)
        .map(|m| m.permissions().mode() & 0o777)
        .unwrap_or(0)
}

fn temp() -> tempfile::TempDir {
    match tempfile::tempdir() {
        Ok(dir) => dir,
        Err(error) => panic!("no temp dir: {error}"),
    }
}

fn open(path: &Path) -> Store {
    match Store::open(path) {
        Ok(store) => store,
        Err(error) => panic!("store refused: {error}"),
    }
}

async fn pragma(store: &Store, name: &'static str) -> String {
    let value = store
        .call(move |conn| {
            Ok(conn.query_row(&format!("PRAGMA {name}"), [], |row| {
                row.get::<_, rusqlite::types::Value>(0)
            })?)
        })
        .await;
    match value {
        Ok(rusqlite::types::Value::Text(text)) => text,
        Ok(rusqlite::types::Value::Integer(n)) => n.to_string(),
        other => panic!("PRAGMA {name}: {other:?}"),
    }
}

// spec: pi-runtime/durable-storage
#[tokio::test]
async fn database_uses_wal_full_sync_and_current_schema() {
    let tmp = temp();
    let store = open(&tmp.path().join("pi.sqlite"));

    assert_eq!(pragma(&store, "journal_mode").await, "wal");
    // 2 = FULL: every commit is fsynced before it returns.
    assert_eq!(pragma(&store, "synchronous").await, "2");
    assert_eq!(pragma(&store, "foreign_keys").await, "1");
    assert_eq!(
        pragma(&store, "user_version").await,
        SCHEMA_VERSION.to_string()
    );
}

// spec: pi-runtime/durable-storage
#[tokio::test]
async fn database_files_have_mode_0600() {
    let tmp = temp();
    let path = tmp.path().join("data").join("pi.sqlite");
    let store = open(&path);
    // A write creates the WAL file.
    let written = store
        .call(|conn| {
            conn.execute("INSERT INTO meta (key, value) VALUES ('probe', 'x')", [])?;
            Ok(())
        })
        .await;
    assert!(written.is_ok(), "{written:?}");

    assert_eq!(mode(&path), 0o600);
    let wal = tmp.path().join("data").join("pi.sqlite-wal");
    assert!(wal.exists());
    assert_eq!(mode(&wal), 0o600);
}

// spec: pi-runtime/durable-storage
#[tokio::test]
async fn existing_file_with_loose_mode_is_tightened() {
    let tmp = temp();
    let path = tmp.path().join("pi.sqlite");
    fs::write(&path, b"").ok();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).ok();
    let _store = open(&path);
    assert_eq!(mode(&path), 0o600);
}

// spec: pi-runtime/durable-storage
#[tokio::test]
async fn committed_data_survives_reopening_and_migrations_run_once() {
    let tmp = temp();
    let path = tmp.path().join("pi.sqlite");
    {
        let store = open(&path);
        let written = store
            .call(|conn| {
                let tx = conn.transaction()?;
                tx.execute(
                    "INSERT INTO meta (key, value) VALUES (?1, ?2)",
                    params!["probe", "kept"],
                )?;
                tx.commit()?;
                Ok(())
            })
            .await;
        assert!(written.is_ok(), "{written:?}");
    }
    let store = open(&path);
    let value = store
        .call(|conn| {
            Ok(
                conn.query_row("SELECT value FROM meta WHERE key = 'probe'", [], |row| {
                    row.get::<_, String>(0)
                })?,
            )
        })
        .await;
    assert_eq!(value.ok().as_deref(), Some("kept"));
    assert_eq!(
        pragma(&store, "user_version").await,
        SCHEMA_VERSION.to_string()
    );
}

// spec: pi-runtime/durable-storage
#[tokio::test]
async fn newer_schema_is_refused() {
    let tmp = temp();
    let path = tmp.path().join("pi.sqlite");
    {
        let store = open(&path);
        let set = store
            .call(|conn| Ok(conn.pragma_update(None, "user_version", 999)?))
            .await;
        assert!(set.is_ok());
    }
    let result = Store::open(&path);
    assert!(
        matches!(result, Err(StoreError::NewerSchema { found: 999, .. })),
        "{result:?}"
    );
}

// spec: pi-runtime/durable-storage
#[tokio::test]
async fn jobs_run_one_at_a_time_in_order() {
    // The single writer (design D4/D5): concurrent callers are serialized.
    let tmp = temp();
    let store = open(&tmp.path().join("pi.sqlite"));
    let created = store
        .call(|conn| Ok(conn.execute_batch("CREATE TABLE log (n INTEGER) STRICT")?))
        .await;
    assert!(created.is_ok());

    let mut tasks = Vec::new();
    for n in 0..50_i64 {
        let store = store.clone();
        tasks.push(tokio::spawn(async move {
            store
                .call(move |conn| {
                    // Read-then-write without a transaction is only safe with
                    // one writer.
                    let count: i64 =
                        conn.query_row("SELECT count(*) FROM log", [], |row| row.get(0))?;
                    conn.execute("INSERT INTO log (n) VALUES (?1)", params![count])?;
                    Ok(n)
                })
                .await
        }));
    }
    for task in tasks {
        assert!(matches!(task.await, Ok(Ok(_))));
    }
    let distinct = store
        .call(|conn| {
            Ok(
                conn.query_row("SELECT count(DISTINCT n) FROM log", [], |row| {
                    row.get::<_, i64>(0)
                })?,
            )
        })
        .await;
    assert_eq!(distinct.ok(), Some(50));
}

// spec: pi-runtime/graceful-stop
#[tokio::test]
async fn close_runs_queued_jobs_then_closes_the_database() {
    let tmp = temp();
    let path = tmp.path().join("pi.sqlite");
    let store = open(&path);
    // Queued before the close, so it commits before the database closes.
    let write = {
        let store = store.clone();
        tokio::spawn(async move {
            store
                .call(|conn| {
                    conn.execute_batch("CREATE TABLE t (x INTEGER); INSERT INTO t VALUES (7);")?;
                    Ok(())
                })
                .await
        })
    };
    tokio::task::yield_now().await;
    let closed = store.close().await;
    assert!(closed.is_ok(), "{closed:?}");
    assert!(matches!(write.await, Ok(Ok(()))));
    // Closed for every handle, and SQLite removed the WAL at the clean close.
    assert!(matches!(
        store.call(|_| Ok(())).await,
        Err(StoreError::Closed)
    ));
    assert!(!tmp.path().join("pi.sqlite-wal").exists());

    let store = open(&path);
    let value = store
        .call(|conn| Ok(conn.query_row("SELECT x FROM t", [], |row| row.get::<_, i64>(0))?))
        .await;
    assert!(matches!(value, Ok(7)), "{value:?}");
}
