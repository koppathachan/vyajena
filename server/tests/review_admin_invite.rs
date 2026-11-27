//! review-admin-invite: the store reviewers' server keeps one reusable admin
//! invite; each `Devices.Register` with it takes over the admin seat.

mod support;

use rusqlite::{OptionalExtension, params};
use server::runtime::review_invite::{self, ReviewInviteError};
use server::runtime::{Bootstrap, Config, ConfigError, Server};
use support::clients::{TestDevice, TestUser};
use support::harness::{Conn, REVIEW_CONFIG, TestServer, authed};
use support::{Check, create_invite, invite_user, register, server_external_sender};

/// The admin's pinned identity key.
async fn admin_key(server: &TestServer) -> Vec<u8> {
    server
        .query(|conn| {
            conn.query_row(
                "SELECT identity_public_key FROM users WHERE username = 'akhil'",
                [],
                |row| row.get(0),
            )
        })
        .await
}

/// Whether the system lobby row exists.
async fn lobby_exists(server: &TestServer) -> bool {
    server
        .query(|conn| {
            conn.query_row(
                "SELECT group_id FROM mls_groups WHERE kind = 'system'",
                [],
                |row| row.get::<_, Vec<u8>>(0),
            )
            .optional()
        })
        .await
        .is_some()
}

/// A reviewer joins as the admin with the reusable invite and authenticates.
async fn join(server: &TestServer, conn: &Conn) -> (TestUser, TestDevice) {
    let reviewer = TestUser::new("akhil");
    let mut device = reviewer.new_device();
    register(conn, &server.bootstrap_code, &reviewer, &device)
        .await
        .check("Register with the reusable invite");
    device.authenticate(conn).await;
    (reviewer, device)
}

/// `Admin.CreateGroup` for the system lobby from `admin`.
async fn create_lobby(conn: &Conn, admin: &mut TestDevice) -> Result<(), tonic::Status> {
    let sender = server_external_sender(conn).await;
    let request = admin.create_group(b"review-lobby-0001", &sender, "akhil@chat");
    conn.admin()
        .create_group(authed(admin.token(), request))
        .await
        .map(|_| ())
}

// spec: device-identity/reusable-review-admin-invite
#[tokio::test]
async fn reviewer_joins_as_the_admin_and_the_invite_stays_usable() {
    let server = TestServer::start_review().await;
    let conn = server.connect().await;
    let (_reviewer, mut device) = join(&server, &conn).await;

    create_lobby(&conn, &mut device).await.check("CreateGroup");
    assert!(lobby_exists(&server).await, "the lobby is seeded");

    let used: Option<i64> = {
        let code = server.bootstrap_code.clone();
        server
            .query(move |conn| {
                conn.query_row(
                    "SELECT used_at FROM invites WHERE code = ?1",
                    params![code],
                    |row| row.get(0),
                )
            })
            .await
    };
    assert_eq!(used, None, "the reusable invite is never marked used");
}

// spec: device-identity/reusable-review-admin-invite
#[tokio::test]
async fn second_reviewer_takes_over_the_admin_seat() {
    let server = TestServer::start_review().await;
    let conn = server.connect().await;
    let (_alice, mut alice_device) = join(&server, &conn).await;
    create_lobby(&conn, &mut alice_device)
        .await
        .check("Alice's lobby");

    let mut revocations = server.state.revocations.subscribe();
    let (bob, mut bob_device) = join(&server, &conn).await;

    // Alice's device is revoked and her stream is told to close.
    assert_eq!(revocations.try_recv().ok(), Some(alice_device.device_id));
    let alice = alice_device
        .try_authenticate_with(&conn, &conn.exporter)
        .await;
    assert_eq!(
        alice.err().map(|s| s.code()),
        Some(tonic::Code::PermissionDenied),
        "the pushed-out reviewer cannot authenticate"
    );

    // Bob's key is the admin's key, and the old lobby is gone so his
    // CreateGroup seeds a fresh one.
    assert_eq!(admin_key(&server).await, bob.identity_public_key());
    assert!(
        !lobby_exists(&server).await,
        "the takeover re-seeded the lobby"
    );
    create_lobby(&conn, &mut bob_device)
        .await
        .check("Bob's lobby");
    create_invite(&conn, &bob_device, "carol").await;
}

// spec: device-identity/reusable-review-admin-invite
#[tokio::test]
async fn same_reviewer_re_joins_after_a_reinstall() {
    let server = TestServer::start_review().await;
    let conn = server.connect().await;
    let (_first, old_device) = join(&server, &conn).await;
    // A reinstall is a new identity key under the same username.
    let (again, new_device) = join(&server, &conn).await;

    assert_eq!(admin_key(&server).await, again.identity_public_key());
    create_invite(&conn, &new_device, "carol").await;
    let old = old_device
        .try_authenticate_with(&conn, &conn.exporter)
        .await;
    assert_eq!(
        old.err().map(|s| s.code()),
        Some(tonic::Code::PermissionDenied)
    );
}

// spec: device-identity/reusable-review-admin-invite
#[tokio::test]
async fn members_are_re_announced_after_a_takeover() {
    let server = TestServer::start_review().await;
    let conn = server.connect().await;
    let (_alice, admin_device) = join(&server, &conn).await;
    let (_carol, carol_device) = invite_user(&conn, &admin_device, "carol").await;
    let carol_id = carol_device.device_id.to_vec();

    // Carol is a lobby member.
    {
        let (carol, admin) = (carol_id.clone(), admin_device.device_id.to_vec());
        server
            .query(move |conn| {
                conn.execute(
                    "INSERT INTO mls_groups (group_id, kind, epoch, pi_sender_index, created_at)
                     VALUES (?1, 'system', 1, 0, 0)",
                    params![b"review-lobby-0001".to_vec()],
                )?;
                conn.execute(
                    "INSERT INTO mls_members (group_id, leaf_index, device_id) VALUES (?1, 0, ?2)",
                    params![b"review-lobby-0001".to_vec(), admin],
                )?;
                conn.execute(
                    "INSERT INTO mls_members (group_id, leaf_index, device_id) VALUES (?1, 1, ?2)",
                    params![b"review-lobby-0001".to_vec(), carol],
                )?;
                conn.execute(
                    "UPDATE devices SET was_member = 1 WHERE device_id = ?1",
                    params![carol],
                )?;
                Ok(())
            })
            .await;
    }

    join(&server, &conn).await;

    let was_member: bool = server
        .query(move |conn| {
            conn.query_row(
                "SELECT was_member FROM devices WHERE device_id = ?1",
                params![carol_id],
                |row| row.get(0),
            )
        })
        .await;
    assert!(!lobby_exists(&server).await, "the lobby was re-seeded");
    assert!(!was_member, "Carol waits for her first add again");
}

// spec: device-identity/reusable-review-admin-invite
#[tokio::test]
async fn ordinary_invites_stay_single_use_on_the_review_server() {
    let server = TestServer::start_review().await;
    let conn = server.connect().await;
    let (_reviewer, admin_device) = join(&server, &conn).await;
    let code = create_invite(&conn, &admin_device, "carol")
        .await
        .invite
        .check("carol's invite")
        .code;

    let carol = TestUser::new("carol");
    register(&conn, &code, &carol, &carol.new_device())
        .await
        .check("Carol registers");
    let again = TestUser::new("carol");
    let second = register(&conn, &code, &again, &again.new_device()).await;
    assert_eq!(
        second.err().map(|s| s.code()),
        Some(tonic::Code::PermissionDenied)
    );
}

// spec: device-identity/reusable-review-admin-invite
#[tokio::test]
async fn flag_off_means_no_reusable_invite() {
    let dir = tempfile::tempdir().check("temp dir");
    let clock = || std::sync::Arc::new(support::manual_clock());
    let review = Server::start(support::config_with(dir.path(), REVIEW_CONFIG), clock())
        .await
        .check("review start");
    assert!(matches!(review.bootstrap(), Bootstrap::Reusable(_)));
    drop(review);
    // The same data, flag off: an ordinary bootstrap, no reusable invite.
    let plain = Server::start(support::config_in(dir.path()), clock())
        .await
        .check("plain start");
    assert!(
        matches!(plain.bootstrap(), Bootstrap::Created(_)),
        "a flag-off start bootstraps as usual: {:?}",
        plain.bootstrap()
    );
}

// spec: device-identity/reusable-review-admin-invite
#[tokio::test]
async fn a_normal_server_rejects_a_reusable_code() {
    let server = TestServer::start().await;
    assert!(
        !server.state.config.review.reusable_admin_invite,
        "flag off by default"
    );
    // Plant a reusable row on a normal server: it must be unknown.
    server
        .query(|conn| {
            conn.execute(
                "INSERT INTO invites (code, username, created_at, expires_at, bootstrap, reusable)
                 VALUES ('reusablecode0000000001', 'akhil', 0, ?1, 0, 1)",
                params![i64::MAX],
            )?;
            Ok(())
        })
        .await;
    let conn = server.connect().await;
    let user = TestUser::new("akhil");
    let result = register(&conn, "reusablecode0000000001", &user, &user.new_device()).await;
    assert_eq!(
        result.err().map(|s| s.code()),
        Some(tonic::Code::PermissionDenied)
    );
    let count: i64 = server
        .query(|conn| conn.query_row("SELECT count(*) FROM devices", [], |row| row.get(0)))
        .await;
    assert_eq!(count, 0, "nothing changed");
}

// spec: device-identity/bootstrap-admin
#[tokio::test]
async fn review_server_mints_no_bootstrap_invite() {
    let mut server = TestServer::start_review().await;
    let first = server.bootstrap_code.clone();
    let bootstraps: i64 = server
        .query(|conn| {
            conn.query_row(
                "SELECT count(*) FROM invites WHERE bootstrap = 1",
                [],
                |row| row.get(0),
            )
        })
        .await;
    assert_eq!(bootstraps, 0, "no bootstrap invite");
    // The same reusable invite survives a restart.
    server.restart().await;
    assert_eq!(server.bootstrap_code, first);
}

// spec: admin-recovery/reset-needs-genuine-host-access-never-a-network-endpoint
#[tokio::test]
async fn only_the_reusable_code_re_seats_the_admin() {
    let server = TestServer::start_review().await;
    let conn = server.connect().await;
    let (alice, alice_device) = join(&server, &conn).await;

    // A made-up code and Admin.CreateInvite for the admin do not re-seat it.
    let mallory = TestUser::new("akhil");
    let made_up = register(
        &conn,
        "notthereusablecode0001",
        &mallory,
        &mallory.new_device(),
    )
    .await;
    assert_eq!(
        made_up.err().map(|s| s.code()),
        Some(tonic::Code::PermissionDenied)
    );
    assert_eq!(admin_key(&server).await, alice.identity_public_key());
    let still = alice_device
        .try_authenticate_with(&conn, &conn.exporter)
        .await;
    assert!(still.is_ok(), "Alice keeps the seat");
}

// spec: server-runtime/configuration
#[test]
fn review_table_parses_and_refuses_unknown_keys() {
    let dir = tempfile::tempdir().check("temp dir");
    let on = support::config_with(dir.path(), REVIEW_CONFIG);
    assert!(on.review.reusable_admin_invite);
    let off = support::config_in(dir.path());
    assert!(!off.review.reusable_admin_invite);

    let unknown = Config::from_toml(
        &format!("{}\n[review]\nopen = true\n", support::config_text()),
        dir.path(),
    );
    assert!(
        matches!(&unknown, Err(ConfigError::Unknown { field }) if field == "review.open"),
        "{unknown:?}"
    );
    let wrong_type = Config::from_toml(
        &format!(
            "{}\n[review]\nreusable_admin_invite = \"yes\"\n",
            support::config_text()
        ),
        dir.path(),
    );
    assert_eq!(
        wrong_type.err().as_ref().and_then(ConfigError::field),
        Some("review.reusable_admin_invite")
    );
}

// spec: server-deployment/review-admin-invite-hand-off
#[tokio::test]
async fn review_invite_prints_the_same_invite_the_server_holds() {
    let server = TestServer::start_review().await;
    let invite = review_invite::read(&server.state.config, server.clock_now())
        .await
        .check("review-invite");
    assert_eq!(invite.invite.code, server.bootstrap_code);
    assert_eq!(invite.payload.relay_address, "relay.example:443");
    assert_eq!(
        invite.payload.tls_spki_sha256,
        server.state.keys.tls_spki_sha256().to_vec()
    );
}

// spec: server-deployment/review-admin-invite-hand-off
#[tokio::test]
async fn review_invite_refuses_on_a_normal_server() {
    let server = TestServer::start().await;
    let error = review_invite::read(&server.state.config, server.clock_now())
        .await
        .err()
        .check("refused");
    assert!(matches!(error, ReviewInviteError::FlagOff), "{error:?}");
}

// spec: server-deployment/review-admin-invite-hand-off
#[tokio::test]
async fn review_invite_refuses_without_a_database() {
    let dir = tempfile::tempdir().check("temp dir");
    std::fs::create_dir(dir.path().join("keys")).check("key dir");
    let config = support::config_with(dir.path(), REVIEW_CONFIG);
    let error = review_invite::read(&config, support::test_epoch())
        .await
        .err()
        .check("refused");
    assert!(
        matches!(error, ReviewInviteError::OpenDatabase(_)),
        "{error:?}"
    );
    assert!(!config.database_path.exists(), "no database was made");
}
