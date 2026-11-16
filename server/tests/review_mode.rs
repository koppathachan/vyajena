//! review-server: Review mode; device-identity: Invites, Signup with an invite,
//! Bootstrap admin (the review exception, and the real server unchanged);
//! pi-runtime: Configuration (`review_mode`).

mod support;

use std::path::Path;
use std::time::Duration;

use protocol::device_identity::INVITE_TTL;
use server::review::KEPT_TABLES;
use server::runtime::{Config, ConfigError};
use support::clients::{TestDevice, TestUser};
use support::harness::{Conn, TestPi, authed};
use support::messaging::Stream;
use support::{
    Check, bootstrap_admin, create_invite, invite_user, pb, pi_external_sender, register,
};
use tonic::Code;

const GROUP_ID: &[u8] = b"the-review-group";

/// 9999-12-31T23:59:59Z in Unix seconds.
const NEVER_SECS: i64 = 253_402_300_799;

/// Row counts of every table, by name.
async fn row_counts(pi: &TestPi) -> Vec<(String, i64)> {
    pi.query(|db| {
        let names: Vec<String> = db
            .prepare(
                "SELECT name FROM sqlite_master
                 WHERE type = 'table' AND name NOT LIKE 'sqlite_%' ORDER BY name",
            )?
            .query_map([], |row| row.get(0))?
            .collect::<Result<_, _>>()?;
        let mut counts = Vec::new();
        for name in names {
            let n: i64 = db.query_row(&format!("SELECT count(*) FROM \"{name}\""), [], |row| {
                row.get(0)
            })?;
            counts.push((name, n));
        }
        Ok(counts)
    })
    .await
}

fn count(counts: &[(String, i64)], table: &str) -> i64 {
    counts
        .iter()
        .find(|(name, _)| name == table)
        .map(|(_, n)| *n)
        .check(table)
}

async fn server_id_in_db(pi: &TestPi) -> String {
    pi.query(|db| {
        db.query_row("SELECT value FROM meta WHERE key = 'server_id'", [], |r| {
            r.get(0)
        })
    })
    .await
}

/// The admin's device creates the group over gRPC.
async fn create_group(conn: &Conn, admin: &mut TestDevice) -> Result<(), tonic::Status> {
    let pi_sender = pi_external_sender(conn).await;
    let request = admin.create_group(GROUP_ID, &pi_sender, "akhil@pi");
    conn.admin()
        .create_group(authed(admin.token(), request))
        .await
        .map(drop)
}

/// `Admin.CreateInvite` for `username`, as a result.
async fn try_invite(
    conn: &Conn,
    admin: &TestDevice,
    username: &str,
) -> Result<pb::CreateInviteResponse, tonic::Status> {
    conn.admin()
        .create_invite(authed(
            admin.token(),
            pb::CreateInviteRequest {
                username: username.to_owned(),
            },
        ))
        .await
        .map(tonic::Response::into_inner)
}

/// A new reviewer signs up with the bootstrap invite and authenticates.
async fn new_reviewer(pi: &TestPi, conn: &Conn) -> (TestUser, TestDevice) {
    let reviewer = TestUser::new("akhil");
    let mut device = reviewer.new_device();
    register(conn, &pi.bootstrap_code, &reviewer, &device)
        .await
        .check("Register with the used review bootstrap invite");
    device.authenticate(conn).await;
    (reviewer, device)
}

/// A review in progress: the admin made the group, invited `bob`, and both
/// uploaded KeyPackages.
async fn a_review_in_progress(pi: &TestPi, conn: &Conn) -> (TestDevice, TestDevice) {
    let (_, mut admin) = bootstrap_admin(pi, conn).await;
    create_group(conn, &mut admin).await.check("CreateGroup");
    let (_, bob) = invite_user(conn, &admin, "bob").await;
    admin.upload_key_packages(conn, 2).await;
    bob.upload_key_packages(conn, 2).await;
    (admin, bob)
}

// spec: review-server/review-mode
#[tokio::test]
async fn a_new_review_empties_the_server_but_keeps_its_id_pin_and_invite() {
    let pi = TestPi::start_review().await;
    let conn = pi.connect().await;
    a_review_in_progress(&pi, &conn).await;
    let before = row_counts(&pi).await;
    for table in [
        "users",
        "devices",
        "sessions",
        "key_packages",
        "mls_group",
        "mls_members",
    ] {
        assert!(
            count(&before, table) > 0,
            "{table} should hold the old review"
        );
    }

    let (_, mut reviewer) = new_reviewer(&pi, &conn).await;

    // Only the new admin is left, and the one bootstrap invite.
    let after = row_counts(&pi).await;
    for (table, n) in &after {
        let expected = match table.as_str() {
            "meta" | "sequence" | "users" | "devices" | "invites" => 1,
            // The new reviewer's own session.
            "sessions" => 1,
            _ => 0,
        };
        assert_eq!(*n, expected, "{table} after a fresh review");
    }
    let invite: (String, i64) = pi
        .query(|db| {
            db.query_row("SELECT code, bootstrap FROM invites", [], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
        })
        .await;
    assert_eq!(invite, (pi.bootstrap_code.clone(), 1));
    let seq: i64 = pi
        .query(|db| db.query_row("SELECT last_server_seq FROM sequence", [], |r| r.get(0)))
        .await;
    assert_eq!(seq, 0);
    assert_eq!(server_id_in_db(&pi).await, "pi");
    // Same keys, same pin: a pinned connection still works.
    assert_eq!(pi.state.keys.tls_spki_sha256(), pi.pin);
    let again = pi.connect().await;

    // The reviewer is the admin and makes a new group.
    let me = again
        .devices()
        .get_self(authed(reviewer.token(), pb::GetSelfRequest {}))
        .await
        .check("GetSelf")
        .into_inner();
    assert!(me.is_admin);
    create_group(&again, &mut reviewer)
        .await
        .check("CreateGroup in the new review");
    // And can invite one user again.
    invite_user(&again, &reviewer, "bob").await;
}

// spec: review-server/review-mode
#[tokio::test]
async fn every_table_but_the_kept_ones_is_wiped() {
    // A table added by a later migration is wiped without being listed.
    let pi = TestPi::start_review().await;
    let conn = pi.connect().await;
    a_review_in_progress(&pi, &conn).await;
    pi.query(|db| {
        db.execute_batch(
            "CREATE TABLE later_feature (x INTEGER) STRICT;
             INSERT INTO later_feature VALUES (1);",
        )
    })
    .await;
    new_reviewer(&pi, &conn).await;
    let after = row_counts(&pi).await;
    assert_eq!(count(&after, "later_feature"), 0);
    assert_eq!(KEPT_TABLES, ["meta", "invites", "sequence"]);
}

// spec: review-server/review-mode
#[tokio::test]
async fn a_bad_signup_with_the_bootstrap_invite_wipes_nothing() {
    let pi = TestPi::start_review().await;
    let conn = pi.connect().await;
    a_review_in_progress(&pi, &conn).await;
    let before = row_counts(&pi).await;
    // A certificate for another user than the invite's.
    let mallory = TestUser::new("mallory");
    let device = mallory.new_device();
    let refused = register(&conn, &pi.bootstrap_code, &mallory, &device).await;
    assert_eq!(
        refused.err().map(|s| s.code()),
        Some(Code::PermissionDenied)
    );
    assert_eq!(row_counts(&pi).await, before);
}

// spec: review-server/review-mode
#[tokio::test]
async fn the_previous_reviews_devices_are_disconnected() {
    let pi = TestPi::start_review().await;
    let conn = pi.connect().await;
    let (admin, bob) = a_review_in_progress(&pi, &conn).await;
    let mut admin_stream = Stream::open(&conn, &admin).await;
    let mut bob_stream = Stream::open(&conn, &bob).await;

    new_reviewer(&pi, &conn).await;

    for stream in [&mut admin_stream, &mut bob_stream] {
        let ended = stream.ended().await;
        assert_eq!(ended.err().map(|s| s.code()), Some(Code::PermissionDenied));
    }
    // Their sessions are gone and the Pi no longer knows them.
    let opened = Stream::open_with(&conn, admin.token(), None).await;
    assert_eq!(opened.err().map(|s| s.code()), Some(Code::Unauthenticated));
    for device in [&admin, &bob] {
        let again = device.try_authenticate_with(&conn, &conn.exporter).await;
        assert_eq!(again.err().map(|s| s.code()), Some(Code::Unauthenticated));
    }
}

// spec: review-server/review-mode
#[tokio::test]
async fn the_review_server_holds_the_admin_and_one_other_user() {
    let pi = TestPi::start_review().await;
    let conn = pi.connect().await;
    let (_, admin) = bootstrap_admin(&pi, &conn).await;
    let refused = |result: Result<pb::CreateInviteResponse, tonic::Status>| {
        let status = result.err().check("CreateInvite should be refused");
        assert_eq!(status.code(), Code::FailedPrecondition);
        assert!(
            status.message().contains("one other user"),
            "{}",
            status.message()
        );
    };

    // A pending invite takes the one slot; revoking it frees it.
    let bob = create_invite(&conn, &admin, "bob")
        .await
        .invite
        .check("invite");
    refused(try_invite(&conn, &admin, "carol").await);
    conn.admin()
        .revoke_invite(authed(
            admin.token(),
            pb::RevokeInviteRequest { code: bob.code },
        ))
        .await
        .check("RevokeInvite");

    // A user takes it; removing the user frees it.
    let (carol, _) = invite_user(&conn, &admin, "carol").await;
    refused(try_invite(&conn, &admin, "dave").await);
    conn.admin()
        .remove_user(authed(
            admin.token(),
            pb::RemoveUserRequest {
                user_id: Some(carol.pb_user_id()),
            },
        ))
        .await
        .check("RemoveUser");
    try_invite(&conn, &admin, "dave")
        .await
        .check("CreateInvite after the removal");
}

// spec: device-identity/invites
#[tokio::test]
async fn the_review_bootstrap_invite_never_expires_and_cannot_be_revoked() {
    let mut pi = TestPi::start_review().await;
    let expires: i64 = pi
        .query(|db| {
            db.query_row(
                "SELECT expires_at FROM invites WHERE bootstrap = 1",
                [],
                |r| r.get(0),
            )
        })
        .await;
    assert_eq!(expires, NEVER_SECS * 1000);

    let conn = pi.connect().await;
    let (_, admin) = bootstrap_admin(&pi, &conn).await;
    // Used: not listed, and not revocable.
    let listed = conn
        .admin()
        .list_invites(authed(admin.token(), pb::ListInvitesRequest {}))
        .await
        .check("ListInvites")
        .into_inner()
        .invites;
    assert!(listed.is_empty(), "{listed:?}");
    let revoke = conn
        .admin()
        .revoke_invite(authed(
            admin.token(),
            pb::RevokeInviteRequest {
                code: pi.bootstrap_code.clone(),
            },
        ))
        .await;
    assert_eq!(
        revoke.err().map(|s| s.code()),
        Some(Code::FailedPrecondition)
    );

    // Ten years and a restart later it still starts a review.
    pi.clock.advance(Duration::from_secs(10 * 365 * 24 * 3600));
    pi.restart().await;
    let conn = pi.connect().await;
    new_reviewer(&pi, &conn).await;
}

// spec: device-identity/signup-with-an-invite
#[tokio::test]
async fn in_review_mode_other_invites_stay_single_use() {
    let pi = TestPi::start_review().await;
    let conn = pi.connect().await;
    let (_, admin) = bootstrap_admin(&pi, &conn).await;
    let invite = create_invite(&conn, &admin, "bob")
        .await
        .invite
        .check("invite");
    let created = invite.created_at.check("created_at").seconds;
    let expires = invite.expires_at.check("expires_at").seconds;
    assert_eq!(expires - created, INVITE_TTL.as_secs() as i64);
    let bob = TestUser::new("bob");
    register(&conn, &invite.code, &bob, &bob.new_device())
        .await
        .check("Register");
    let bob2 = TestUser::new("bob");
    let again = register(&conn, &invite.code, &bob2, &bob2.new_device()).await;
    assert_eq!(again.err().map(|s| s.code()), Some(Code::PermissionDenied));
}

// spec: device-identity/bootstrap-admin
#[tokio::test]
async fn the_real_servers_bootstrap_invite_is_single_use() {
    let pi = TestPi::start().await;
    assert!(!pi.state.config.review_mode);
    let (created, expires): (i64, i64) = pi
        .query(|db| {
            db.query_row(
                "SELECT created_at, expires_at FROM invites WHERE bootstrap = 1",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
        })
        .await;
    assert_eq!(expires - created, INVITE_TTL.as_millis() as i64);

    let conn = pi.connect().await;
    let (_, admin) = bootstrap_admin(&pi, &conn).await;
    let other = TestUser::new("akhil");
    let again = register(&conn, &pi.bootstrap_code, &other, &other.new_device()).await;
    assert_eq!(again.err().map(|s| s.code()), Some(Code::PermissionDenied));
    // Nothing was wiped: the admin is still there.
    let users: i64 = pi
        .query(|db| db.query_row("SELECT count(*) FROM users", [], |r| r.get(0)))
        .await;
    assert_eq!(users, 1);
    let me = conn
        .devices()
        .get_self(authed(admin.token(), pb::GetSelfRequest {}))
        .await;
    assert!(me.is_ok());
}

// spec: device-identity/bootstrap-admin
#[tokio::test]
async fn the_real_servers_bootstrap_invite_expires_after_7_days() {
    let mut pi = TestPi::start().await;
    pi.clock.advance(INVITE_TTL + Duration::from_secs(1));
    let conn = pi.connect().await;
    let admin = TestUser::new("akhil");
    let refused = register(&conn, &pi.bootstrap_code, &admin, &admin.new_device()).await;
    assert_eq!(
        refused.err().map(|s| s.code()),
        Some(Code::PermissionDenied)
    );
    // The next start makes a new one, as before.
    pi.restart().await;
    let expires: i64 = pi
        .query(|db| {
            db.query_row(
                "SELECT max(expires_at) FROM invites WHERE bootstrap = 1",
                [],
                |r| r.get(0),
            )
        })
        .await;
    assert!(expires < NEVER_SECS * 1000);
}

// spec: review-server/review-mode
#[tokio::test]
async fn the_real_server_has_no_user_cap() {
    let pi = TestPi::start().await;
    let conn = pi.connect().await;
    let (_, admin) = bootstrap_admin(&pi, &conn).await;
    invite_user(&conn, &admin, "bob").await;
    for name in ["carol", "dave", "erin"] {
        try_invite(&conn, &admin, name)
            .await
            .check("CreateInvite on the real server");
    }
}

// spec: review-server/review-mode
#[tokio::test]
async fn a_review_server_start_reports_its_pending_bootstrap_invite() {
    let mut pi = TestPi::start_review().await;
    pi.restart().await;
    // Not printed again, and still the same invite.
    let codes: Vec<String> = pi
        .query(|db| {
            db.prepare("SELECT code FROM invites")?
                .query_map([], |r| r.get(0))?
                .collect()
        })
        .await;
    assert_eq!(codes, [pi.bootstrap_code.clone()]);
}

// spec: pi-runtime/configuration
#[test]
fn review_mode_is_an_optional_bool_off_by_default() {
    let parse = |text: &str| Config::from_toml(text, Path::new("/srv/pi"));
    let plain = parse(&support::config_text()).check("config without review_mode");
    assert!(!plain.review_mode);
    let review = parse(&support::review_config_text()).check("config with review_mode");
    assert!(review.review_mode);
    let off = parse(&format!("{}review_mode = false\n", support::config_text()))
        .check("review_mode = false");
    assert!(!off.review_mode);
    match parse(&format!(
        "{}review_mode = \"yes\"\n",
        support::config_text()
    )) {
        Err(error @ ConfigError::Invalid { .. }) => {
            assert_eq!(error.field(), Some("review_mode"));
        }
        other => panic!("review_mode = \"yes\" should be invalid, got {other:?}"),
    }
}
