//! dev-onboarding: Dev onboarding mode; Server advertises its onboarding mode;
//! Open registration; Join code; Default-admin auto-claim; Maximum users.

mod support;

use std::path::Path;

use server::runtime::config::{Config, ConfigError, OnboardingMode};
use support::binary::Home;
use support::clients::TestUser;
use support::harness::{Blobs, TestServer, authed};
use support::{Check, link_device, pb, register, register_open};
use tonic::Code;

fn parse(text: &str) -> Result<Config, ConfigError> {
    Config::from_toml(text, Path::new("/srv/server"))
}

/// The base test config with an `[onboarding]` table appended.
fn with_onboarding(table: &str) -> String {
    format!("{}\n{table}\n", support::config_text())
}

/// Starts a server in open onboarding mode with `extra` onboarding lines.
async fn open_server(extra: &str) -> TestServer {
    TestServer::start_with_blobs_and_config(
        Blobs::Off,
        &format!("[onboarding]\nmode = \"open\"\n{extra}"),
    )
    .await
}

async fn user_count(server: &TestServer) -> i64 {
    server
        .query(|db| {
            db.query_row(
                "SELECT count(*) FROM users WHERE removed_at IS NULL",
                [],
                |r| r.get(0),
            )
        })
        .await
}

// --- Config parsing (server-runtime: Dev onboarding mode) ---

// spec: server-runtime/dev-onboarding-mode
#[test]
fn default_is_invite_only() {
    let config = parse(&support::config_text()).check("parse");
    assert_eq!(config.onboarding.mode, OnboardingMode::Invite);
    assert_eq!(config.onboarding.join_code, None);
    assert_eq!(config.onboarding.max_users, None);
}

// spec: server-runtime/dev-onboarding-mode
#[test]
fn open_mode_with_join_code_and_cap_parses() {
    let config = parse(&with_onboarding(
        "[onboarding]\nmode = \"open\"\njoin_code = \"letmein\"\nmax_users = 10",
    ))
    .check("parse");
    assert_eq!(config.onboarding.mode, OnboardingMode::Open);
    assert_eq!(config.onboarding.join_code.as_deref(), Some("letmein"));
    assert_eq!(config.onboarding.max_users, Some(10));
}

// spec: server-runtime/dev-onboarding-mode
#[test]
fn a_bad_onboarding_setting_names_the_field() {
    let cases = [
        ("[onboarding]\nmode = \"bogus\"", "onboarding.mode"),
        ("[onboarding]\nmode = 1", "onboarding.mode"),
        ("[onboarding]\njoin_code = \"\"", "onboarding.join_code"),
        ("[onboarding]\njoin_code = 5", "onboarding.join_code"),
        ("[onboarding]\nmax_users = 0", "onboarding.max_users"),
        ("[onboarding]\nmax_users = -1", "onboarding.max_users"),
        ("[onboarding]\nmax_users = \"x\"", "onboarding.max_users"),
        ("[onboarding]\nbogus = 1", "onboarding.bogus"),
        ("onboarding = 5", "onboarding"),
    ];
    for (table, field) in cases {
        match parse(&with_onboarding(table)) {
            Err(error) => assert_eq!(error.field(), Some(field), "for {table}: {error}"),
            Ok(_) => panic!("config accepted: {table}"),
        }
    }
}

// spec: server-runtime/dev-onboarding-mode
#[tokio::test]
async fn open_mode_warns_at_startup_and_never_logs_the_join_code() {
    let home = Home::new();
    std::fs::write(
        home.config(),
        with_onboarding(
            "[onboarding]\nmode = \"open\"\njoin_code = \"s3cret-join\"\nmax_users = 5",
        ),
    )
    .check("write server.toml");
    let server = home.start();

    // A loud open-mode warning, and the effective mode, are logged...
    assert!(
        server.log.iter().any(|l| l.contains("open mode")),
        "no open-mode warning in the log: {:#?}",
        server.log
    );
    assert!(
        server.log.iter().any(|l| l.contains("onboarding_mode")),
        "the effective onboarding mode is not logged"
    );
    // ...but the join code value itself is NEVER logged.
    assert!(
        server.log.iter().all(|l| !l.contains("s3cret-join")),
        "the join code leaked into the log: {:#?}",
        server.log
    );
}

// --- Server advertisement (server-runtime: Server advertises its onboarding mode) ---

// spec: server-runtime/server-advertises-its-onboarding-mode
#[tokio::test]
async fn open_server_advertises_open_mode() {
    let server = open_server("join_code = \"letmein\"\nmax_users = 10\n").await;
    let conn = server.connect().await;
    let info = conn
        .devices()
        .get_server_info(pb::GetServerInfoRequest {})
        .await
        .check("GetServerInfo")
        .into_inner()
        .server_info
        .check("server_info");
    let onboarding = info.onboarding.check("onboarding");
    assert_eq!(onboarding.mode, pb::onboarding::Mode::Open as i32);
    assert!(onboarding.join_code_required);
    assert_eq!(onboarding.max_users, 10);
    assert_eq!(onboarding.admin_username, "akhil");
}

// spec: server-runtime/server-advertises-its-onboarding-mode
#[tokio::test]
async fn invite_server_advertises_invite_mode() {
    let server = TestServer::start().await;
    let conn = server.connect().await;
    let onboarding = conn
        .devices()
        .get_server_info(pb::GetServerInfoRequest {})
        .await
        .check("GetServerInfo")
        .into_inner()
        .server_info
        .check("server_info")
        .onboarding
        .check("onboarding");
    assert_eq!(onboarding.mode, pb::onboarding::Mode::Invite as i32);
    assert!(!onboarding.join_code_required);
    assert_eq!(onboarding.max_users, 0);
}

// --- Open registration (device-identity) ---

// spec: device-identity/signup-with-an-invite
#[tokio::test]
async fn invite_less_register_is_failed_precondition_in_invite_mode() {
    let server = TestServer::start().await;
    let conn = server.connect().await;
    let alice = TestUser::new("alice");
    let result = register_open(&conn, &alice, &alice.new_device(), "").await;
    assert_eq!(
        result.err().map(|s| s.code()),
        Some(Code::FailedPrecondition)
    );
    assert_eq!(user_count(&server).await, 0, "nothing is created");
}

// spec: device-identity/open-registration
#[tokio::test]
async fn open_signup_without_an_invite_creates_user_and_device() {
    let server = open_server("").await;
    let conn = server.connect().await;
    let alice = TestUser::new("alice");
    let mut phone = alice.new_device();
    let response = register_open(&conn, &alice, &phone, "")
        .await
        .check("register_open");
    assert_eq!(response.user_id, Some(alice.pb_user_id()));
    assert_eq!(user_count(&server).await, 1);
    // The device authenticates with its own key, as an invited device does.
    phone.authenticate(&conn).await;

    // An active username cannot be taken again by open registration.
    let mallory = TestUser::new("alice");
    let again = register_open(&conn, &mallory, &mallory.new_device(), "").await;
    assert_eq!(again.err().map(|s| s.code()), Some(Code::AlreadyExists));
}

// spec: device-identity/join-code
#[tokio::test]
async fn join_code_gates_open_registration() {
    let server = open_server("join_code = \"letmein\"\n").await;
    let conn = server.connect().await;

    let bob = TestUser::new("bob");
    let wrong = register_open(&conn, &bob, &bob.new_device(), "nope").await;
    assert_eq!(wrong.err().map(|s| s.code()), Some(Code::PermissionDenied));
    assert_eq!(user_count(&server).await, 0, "a wrong code creates nothing");

    register_open(&conn, &bob, &bob.new_device(), "letmein")
        .await
        .check("register_open with the right code");
    assert_eq!(user_count(&server).await, 1);
}

// spec: device-identity/default-admin-auto-claim
#[tokio::test]
async fn admin_joins_directly_in_open_mode() {
    let server = open_server("").await;
    let conn = server.connect().await;

    // The admin self-registers by choosing the configured admin username.
    let admin = TestUser::new("akhil");
    let mut admin_device = admin.new_device();
    register_open(&conn, &admin, &admin_device, "")
        .await
        .check("admin register_open");
    admin_device.authenticate(&conn).await;
    let me = conn
        .devices()
        .get_self(authed(admin_device.token(), pb::GetSelfRequest {}))
        .await
        .check("GetSelf")
        .into_inner();
    assert!(me.is_admin, "the configured admin username is the admin");

    // An ordinary user is not the admin.
    let carol = TestUser::new("carol");
    let mut carol_device = carol.new_device();
    register_open(&conn, &carol, &carol_device, "")
        .await
        .check("carol register_open");
    carol_device.authenticate(&conn).await;
    let me = conn
        .devices()
        .get_self(authed(carol_device.token(), pb::GetSelfRequest {}))
        .await
        .check("GetSelf")
        .into_inner();
    assert!(!me.is_admin);
}

// spec: device-identity/maximum-users
#[tokio::test]
async fn max_users_caps_new_users_but_not_linking() {
    let server = open_server("max_users = 2\n").await;
    let conn = server.connect().await;

    // Two users fill the cap.
    let a = TestUser::new("a");
    let mut a_phone = a.new_device();
    register_open(&conn, &a, &a_phone, "").await.check("a");
    let b = TestUser::new("b");
    register_open(&conn, &b, &b.new_device(), "")
        .await
        .check("b");
    assert_eq!(user_count(&server).await, 2);

    // A third new user is refused.
    let c = TestUser::new("c");
    let result = register_open(&conn, &c, &c.new_device(), "").await;
    assert_eq!(
        result.err().map(|s| s.code()),
        Some(Code::ResourceExhausted)
    );
    assert_eq!(
        user_count(&server).await,
        2,
        "nothing is created past the cap"
    );

    // Linking another device to an existing user is not a new user, so it is
    // allowed even at the cap.
    a_phone.authenticate(&conn).await;
    link_device(&conn, &a, &a_phone).await;
    assert_eq!(user_count(&server).await, 2, "linking adds no user");
}

// spec: device-identity/signup-with-an-invite
#[tokio::test]
async fn an_invite_still_works_in_open_mode() {
    let server = open_server("").await;
    let conn = server.connect().await;
    // Bring up the admin in open mode, then mint an invite as the admin does.
    let admin = TestUser::new("akhil");
    let mut admin_device = admin.new_device();
    register_open(&conn, &admin, &admin_device, "")
        .await
        .check("admin");
    admin_device.authenticate(&conn).await;
    let code = support::create_invite(&conn, &admin_device, "dave")
        .await
        .invite
        .check("invite")
        .code;
    let dave = TestUser::new("dave");
    register(&conn, &code, &dave, &dave.new_device())
        .await
        .check("invited signup still works in open mode");
}
