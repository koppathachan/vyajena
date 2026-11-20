//! server-deployment: Backup to the Mac. The binary's `backup` subcommand, run
//! while the server is serving (deploy/server/backup.sh runs it on the server).

mod support;

use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::{Command, Output};

use rusqlite::{Connection, OpenFlags};
use support::Check;
use support::binary::{Home, admin_on};
use support::messaging::envelope;
use support::pb::envelope::Kind;

fn backup(config: &Path, to: &Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_server"))
        .arg("backup")
        .arg("--config")
        .arg(config)
        .arg("--to")
        .arg(to)
        .output()
        .check("run server backup")
}

fn count(db: &Connection, table: &str) -> i64 {
    db.query_row(&format!("SELECT count(*) FROM {table}"), [], |row| {
        row.get(0)
    })
    .check(table)
}

// spec: server-deployment/backup-to-the-mac
#[tokio::test]
async fn snapshot_while_alice_chats_is_a_valid_private_database() {
    let home = Home::new();
    let server = home.start();
    let (_conn, _admin, mut device, mut stream) = admin_on(&server).await;
    let epoch = device.epoch();
    let first = stream
        .accepted(envelope(Kind::Application, epoch, device.message(b"hello")))
        .await;

    let snapshot = home.path().join("snapshot.sqlite");
    let out = backup(&home.config(), &snapshot);
    assert!(out.status.success(), "{out:?}");

    // Owner-only, self-contained, and it opens as a valid database with the
    // rows written before it.
    let mode = std::fs::metadata(&snapshot)
        .check("snapshot")
        .permissions()
        .mode();
    assert_eq!(mode & 0o777, 0o600);
    assert!(!home.path().join("snapshot.sqlite-wal").exists());
    let db = Connection::open_with_flags(&snapshot, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .check("open the snapshot");
    let check: String = db
        .query_row("PRAGMA integrity_check", [], |row| row.get(0))
        .check("integrity_check");
    assert_eq!(check, "ok");
    let admin: String = db
        .query_row("SELECT username FROM users", [], |row| row.get(0))
        .check("the admin");
    assert_eq!(admin, "akhil");
    assert!(count(&db, "envelopes") >= 1);
    assert_eq!(count(&db, "mls_groups"), 1);

    // The service kept running.
    let next = stream
        .accepted(envelope(
            Kind::Application,
            epoch,
            device.message(b"still here"),
        ))
        .await;
    assert_eq!(next, first + 1);
}

// spec: server-deployment/backup-to-the-mac
#[test]
fn backup_never_overwrites_a_file() {
    let home = Home::new();
    drop(home.start());
    let target = home.path().join("taken.sqlite");
    std::fs::write(&target, b"keep me").check("write");
    let out = backup(&home.config(), &target);
    assert!(!out.status.success());
    assert_eq!(std::fs::read(&target).check("read"), b"keep me");
}

// spec: server-deployment/backup-to-the-mac
#[test]
fn backup_without_a_database_fails_and_creates_nothing() {
    let home = Home::new();
    let target = home.path().join("snapshot.sqlite");
    let out = backup(&home.config(), &target);
    assert!(!out.status.success());
    assert!(!target.exists());
    assert!(!home.path().join("server.sqlite").exists());
}
