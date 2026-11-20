//! Joining through the bridge (flutter-app task 2.3): register from invite
//! text against the real server, the server identity error, bad invite text.

mod support;

use bridge::api::error::CoreErrorKind;
use bridge::api::text::parse_invite;
use bridge::api::types::CoreEvent;
use support::Check;
use support::device::{self, Device, user};
use support::server::{ADMIN, TestServer};

// spec: app-ui/join-from-an-invite
#[tokio::test(flavor = "multi_thread")]
async fn bob_sees_whom_the_invite_names_then_joins_and_lands_in_the_chat() {
    let server = TestServer::start().await;
    let admin = device::admin_with_group(&server).await;
    assert!(admin.core.is_admin().check("is admin"));

    let invite = admin
        .core
        .create_invite("bob".to_owned())
        .await
        .check("create invite");
    let text = invite.text.clone().check("invite text");
    assert!(text.starts_with("vyajena-invite:"), "{text}");
    assert_eq!(invite.user, user("bob"));

    // What the app shows before joining.
    let preview = parse_invite(text.clone()).check("parse invite");
    assert_eq!(preview.user, user("bob"));
    assert_eq!(preview.relay_address, server.relay.addr.to_string());

    let mut bob = Device::new().await;
    assert_eq!(bob.core.user_id().check("user id"), None);
    let id = bob.core.register(text).await.check("register");
    assert_eq!(id, user("bob"));
    assert_eq!(bob.core.user_id().check("user id"), Some(user("bob")));
    assert!(!bob.core.is_admin().check("is admin"));
    let device = bob.core.device_id().check("device id").check("device id");
    assert_eq!(device.len(), 32);

    bob.core.start().await.check("start");
    bob.wait_connected().await;
    assert!(bob.core.is_connected());
    admin.core.add_user(user("bob")).await.check("add bob");
    bob.wait_event("joined", |e| matches!(e, CoreEvent::JoinedGroup))
        .await;
    assert!(bob.core.epoch().check("epoch").is_some());
}

// spec: app-ui/join-from-an-invite
#[tokio::test(flavor = "multi_thread")]
async fn the_admin_joins_with_the_bootstrap_invite_the_server_printed() {
    let server = TestServer::start().await;
    // The server prints it bare, without the bridge's prefix.
    let preview = parse_invite(device::bootstrap_text(&server)).check("parse bootstrap");
    assert_eq!(preview.user, user(ADMIN));
    let admin = device::admin_with_group(&server).await;
    assert!(admin.core.is_admin().check("is admin"));
}

// spec: app-ui/join-from-an-invite
#[tokio::test(flavor = "multi_thread")]
async fn a_server_with_another_tls_key_is_a_server_identity_error() {
    let mut server = TestServer::start().await;
    server.restart_with_new_keys().await;
    let mut admin = Device::new().await;
    let error = admin
        .core
        .register(device::bootstrap_text(&server))
        .await
        .expect_err("the pin must not match");
    assert_eq!(error.kind, CoreErrorKind::ServerIdentity, "{error:?}");
    assert_eq!(admin.core.user_id().check("user id"), None);
    admin
        .wait_event("the server identity event", |e| {
            matches!(e, CoreEvent::ServerIdentityError)
        })
        .await;
}

// spec: app-ui/join-from-an-invite
#[tokio::test(flavor = "multi_thread")]
async fn text_that_is_not_an_invite_is_refused_before_any_call() {
    for text in ["hello there", "", "vyajena-invite:not base64!", "AAAA"] {
        let error = parse_invite(text.to_owned()).expect_err(text);
        assert_eq!(error.kind, CoreErrorKind::InvalidPayload, "{text}");
    }
    // A link approval pasted into the invite box says what it is.
    let error = parse_invite("vyajena-approval:AAAA".to_owned()).expect_err("approval");
    assert_eq!(error.kind, CoreErrorKind::InvalidPayload);
    assert!(error.message.contains("link approval"), "{}", error.message);

    let bob = Device::new().await;
    let error = bob
        .core
        .register("hello there".to_owned())
        .await
        .expect_err("not an invite");
    assert_eq!(error.kind, CoreErrorKind::InvalidPayload);
    assert_eq!(bob.core.user_id().check("user id"), None);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_used_invite_is_refused_by_the_server() {
    let server = TestServer::start().await;
    let admin = device::admin_with_group(&server).await;
    let error = Device::new()
        .await
        .core
        .register(device::bootstrap_text(&server))
        .await
        .expect_err("the bootstrap invite is used");
    assert_eq!(error.kind, CoreErrorKind::Refused, "{error:?}");
    drop(admin);
}
