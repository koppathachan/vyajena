//! admin-recovery: Operator break-glass reset and the re-bootstrap it enables.

mod support;

use std::path::Path;

use rusqlite::{OptionalExtension, params};
use server::runtime::admin_reset::{self, ResetError};
use server::store::Store;
use support::clients::TestUser;
use support::harness::TestServer;
use support::{Check, bootstrap_admin, create_invite, invite_user, register};

/// `removed_at` of a user, if the user exists.
async fn removed_at(server: &TestServer, username: &'static str) -> Option<Option<i64>> {
    server
        .query(move |conn| {
            conn.query_row(
                "SELECT removed_at FROM users WHERE username = ?1",
                params![username],
                |row| row.get::<_, Option<i64>>(0),
            )
            .optional()
        })
        .await
}

/// Whether every device of `username` is revoked, and how many devices exist.
async fn devices_state(server: &TestServer, username: &'static str) -> (usize, usize) {
    server
        .query(move |conn| {
            let total: usize = conn.query_row(
                "SELECT count(*) FROM devices WHERE username = ?1",
                params![username],
                |row| row.get(0),
            )?;
            let revoked: usize = conn.query_row(
                "SELECT count(*) FROM devices WHERE username = ?1 AND revoked_at IS NOT NULL",
                params![username],
                |row| row.get(0),
            )?;
            Ok((total, revoked))
        })
        .await
}

/// The identity public key pinned for `username`.
async fn identity_key(server: &TestServer, username: &'static str) -> Vec<u8> {
    server
        .query(move |conn| {
            conn.query_row(
                "SELECT identity_public_key FROM users WHERE username = ?1",
                params![username],
                |row| row.get(0),
            )
        })
        .await
}

// spec: admin-recovery/operator-break-glass-reset
#[tokio::test]
async fn operator_runs_the_reset_and_mints_no_invite() {
    let server = TestServer::start().await;
    let conn = server.connect().await;
    let (_admin, _admin_device) = bootstrap_admin(&server, &conn).await;

    let invites_before: usize = server
        .query(|conn| conn.query_row("SELECT count(*) FROM invites", [], |row| row.get(0)))
        .await;

    let report = admin_reset::reset_admin(
        &server.state.store,
        &server.state.config,
        server.clock_now(),
    )
    .await
    .check("admin reset");
    assert_eq!(report.admin, "akhil");

    // The admin is marked removed (non-null removed_at) and its one device revoked.
    assert!(matches!(removed_at(&server, "akhil").await, Some(Some(_))));
    assert_eq!(devices_state(&server, "akhil").await, (1, 1));

    // It minted no invite: the invite count did not grow.
    let invites_after: usize = server
        .query(|conn| conn.query_row("SELECT count(*) FROM invites", [], |row| row.get(0)))
        .await;
    assert_eq!(invites_after, invites_before);
}

// spec: admin-recovery/operator-break-glass-reset
#[tokio::test]
async fn blast_radius_is_one_user() {
    let server = TestServer::start().await;
    let conn = server.connect().await;
    let (_admin, admin_device) = bootstrap_admin(&server, &conn).await;
    let (_alice, _alice_device) = invite_user(&conn, &admin_device, "alice").await;
    let (_bob, _bob_device) = invite_user(&conn, &admin_device, "bob").await;
    // A non-admin unused invite (for carol) must survive the reset.
    let carol_invite = create_invite(&conn, &admin_device, "carol").await;
    let carol_code = carol_invite.invite.check("carol invite").code;

    // An unused invite for the admin itself must be revoked by the reset.
    server
        .query(|conn| {
            conn.execute(
                "INSERT INTO invites (code, username, created_at, expires_at, bootstrap)
                 VALUES (?1, ?2, ?3, ?4, 0)",
                params!["akhiladmininvite000001", "akhil", 0_i64, i64::MAX],
            )?;
            Ok(())
        })
        .await;

    let pin_before = server.state.keys.tls_spki_sha256();
    let report = admin_reset::reset_admin(
        &server.state.store,
        &server.state.config,
        server.clock_now(),
    )
    .await
    .check("admin reset");
    assert_eq!(report.devices_revoked, 1, "the admin's one device");
    assert_eq!(report.invites_revoked, 1, "the admin's one unused invite");

    // The admin is reset.
    assert!(matches!(removed_at(&server, "akhil").await, Some(Some(_))));
    assert_eq!(devices_state(&server, "akhil").await, (1, 1));

    // Everyone else is untouched: active, with live devices.
    for other in ["alice", "bob"] {
        assert_eq!(
            removed_at(&server, other).await,
            Some(None),
            "{other} active"
        );
        let (total, revoked) = devices_state(&server, other).await;
        assert!(total > 0 && revoked == 0, "{other}'s devices live");
    }

    // The non-admin invite survives; the admin's unused invite is revoked.
    let (carol_revoked, admin_invite_revoked): (Option<i64>, Option<i64>) = server
        .query(move |conn| {
            let carol = conn.query_row(
                "SELECT revoked_at FROM invites WHERE code = ?1",
                params![carol_code],
                |row| row.get::<_, Option<i64>>(0),
            )?;
            let admin = conn.query_row(
                "SELECT revoked_at FROM invites WHERE code = 'akhiladmininvite000001'",
                [],
                |row| row.get::<_, Option<i64>>(0),
            )?;
            Ok((carol, admin))
        })
        .await;
    assert_eq!(carol_revoked, None, "carol's invite is untouched");
    assert!(
        admin_invite_revoked.is_some(),
        "the admin's invite is revoked"
    );

    // The TLS pin (server identity) is unchanged.
    assert_eq!(server.state.keys.tls_spki_sha256(), pin_before);
}

// spec: admin-recovery/recovered-admin-re-bootstraps-with-a-fresh-identity
#[tokio::test]
async fn recovered_admin_re_bootstraps_with_a_fresh_identity() {
    let mut server = TestServer::start().await;
    let conn = server.connect().await;
    let (admin, old_device) = bootstrap_admin(&server, &conn).await;
    let old_key = admin.identity_public_key();

    // Operator break-glass reset, then restart to re-mint the bootstrap invite.
    admin_reset::reset_admin(
        &server.state.store,
        &server.state.config,
        server.clock_now(),
    )
    .await
    .check("admin reset");
    drop(conn);
    server.restart().await;

    // The re-mint path: the admin registers a NEW device with a NEW identity key.
    let conn2 = server.connect().await;
    let admin2 = TestUser::new("akhil");
    let new_key = admin2.identity_public_key();
    assert_ne!(new_key, old_key, "the recovered admin re-keys");
    let mut new_device = admin2.new_device();
    register(&conn2, &server.bootstrap_code, &admin2, &new_device)
        .await
        .check("re-register the admin");

    // The server installed the new key and cleared removed_at (re-key path).
    assert_eq!(identity_key(&server, "akhil").await, new_key);
    assert_eq!(removed_at(&server, "akhil").await, Some(None));

    // The admin has authority again: authenticate and call an Admin RPC.
    new_device.authenticate(&conn2).await;
    create_invite(&conn2, &new_device, "dave").await;

    // The old admin device stays locked out: the reset revoked it, so it cannot
    // authenticate even on a fresh connection.
    let old = old_device
        .try_authenticate_with(&conn2, &conn2.exporter)
        .await;
    assert_eq!(
        old.err().map(|s| s.code()),
        Some(tonic::Code::PermissionDenied),
        "the pre-reset admin device cannot authenticate"
    );
}

// spec: admin-recovery/reset-needs-genuine-host-access-never-a-network-endpoint
#[tokio::test]
async fn no_host_access_no_reset() {
    // A config whose database file does not exist: the reset refuses to run and
    // changes nothing (it cannot even open a database).
    let dir = tempfile::tempdir().check("temp dir");
    let config = support::config_in(dir.path());
    let error = admin_reset::run(&config, None, support::test_epoch())
        .await
        .err()
        .check("reset without a database errors");
    assert!(
        matches!(error, ResetError::OpenDatabase(_)),
        "expected OpenDatabase, got {error:?}"
    );
    // Nothing was created where the database would be.
    assert!(!config.database_path.exists(), "no database was made");
}

// spec: admin-recovery/reset-needs-genuine-host-access-never-a-network-endpoint
#[tokio::test]
async fn wrong_or_absent_passphrase_changes_nothing() {
    let dir = tempfile::tempdir().check("temp dir");
    let config = support::config_with(dir.path(), "[recovery]\npassphrase = \"swordfish\"\n");
    // Seed a database with an active admin, then close it so `run` opens it.
    {
        let store = Store::open(&config.database_path).check("open store");
        store
            .call(|conn| {
                conn.execute(
                    "INSERT INTO users (username, identity_public_key, created_at)
                     VALUES ('akhil', ?1, 0)",
                    params![vec![7u8; 32]],
                )?;
                Ok(())
            })
            .await
            .check("seed admin");
        store.close().await.check("close store");
    }
    let still_active = |config: &server::runtime::Config| {
        let path = config.database_path.clone();
        async move {
            let store = Store::open(&path).check("reopen");
            let removed: Option<i64> = store
                .call(|conn| {
                    Ok(conn.query_row(
                        "SELECT removed_at FROM users WHERE username = 'akhil'",
                        [],
                        |row| row.get(0),
                    )?)
                })
                .await
                .check("read removed_at");
            store.close().await.check("close");
            removed
        }
    };

    // Absent passphrase: refused.
    let absent = admin_reset::run(&config, None, support::test_epoch()).await;
    assert!(matches!(absent, Err(ResetError::Passphrase)), "{absent:?}");
    assert_eq!(
        still_active(&config).await,
        None,
        "still active after no passphrase"
    );

    // Wrong passphrase: refused.
    let wrong = admin_reset::run(&config, Some("guess"), support::test_epoch()).await;
    assert!(matches!(wrong, Err(ResetError::Passphrase)), "{wrong:?}");
    assert_eq!(
        still_active(&config).await,
        None,
        "still active after wrong passphrase"
    );

    // Correct passphrase: the reset runs.
    let right = admin_reset::run(&config, Some("swordfish"), support::test_epoch())
        .await
        .check("correct passphrase resets");
    assert_eq!(right.admin, "akhil");
    assert!(still_active(&config).await.is_some(), "now removed");
}

// spec: admin-recovery/reset-needs-genuine-host-access-never-a-network-endpoint
#[test]
fn no_rpc_can_reset_the_admin() {
    // Inventory check: no RPC in the wire contract resets or recovers the admin.
    let proto_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../proto/vyajena/v1");
    let mut rpc_lines = 0usize;
    for entry in std::fs::read_dir(&proto_dir).check("read proto dir") {
        let path = entry.check("dir entry").path();
        if path.extension().and_then(|e| e.to_str()) != Some("proto") {
            continue;
        }
        let text = std::fs::read_to_string(&path).check("read proto file");
        for line in text.lines() {
            let trimmed = line.trim();
            if !trimmed.starts_with("rpc ") {
                continue;
            }
            rpc_lines += 1;
            let lower = trimmed.to_ascii_lowercase();
            assert!(
                !lower.contains("reset") && !lower.contains("recover"),
                "an RPC looks like an admin reset/recover endpoint: {trimmed:?}"
            );
        }
    }
    assert!(rpc_lines > 0, "the inventory check found no RPCs to scan");
}
