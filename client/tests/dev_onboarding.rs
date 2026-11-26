//! dev-onboarding: Open registration in the core.

mod support;

use client::ClientError;
use support::Check;
use support::device::{Device, user};
use support::server::{ADMIN, TestServer};

// spec: client-runtime/open-registration-in-the-core
#[tokio::test]
async fn core_registers_openly() {
    let server = TestServer::start_open().await;
    let device = Device::new().await;
    let relay = server.relay.addr.to_string();
    let pin = server.state.keys.tls_spki_sha256();

    // The app can read the onboarding advertisement to choose its screen.
    let onboarding = device
        .client
        .server_onboarding(&relay, pin)
        .await
        .check("server_onboarding");
    assert!(onboarding.open, "the server advertises open mode");
    assert_eq!(onboarding.admin_username, ADMIN);
    assert!(!onboarding.join_code_required);

    // register_open generates this device's keys and registers with no invite.
    let id = device
        .client
        .register_open(&relay, pin, "alice", "")
        .await
        .check("register_open");
    assert_eq!(id, user("alice"));
    assert_eq!(
        device.client.is_admin().check("is_admin"),
        false,
        "alice is an ordinary user"
    );
}

// spec: client-runtime/open-registration-in-the-core
#[tokio::test]
async fn admin_registers_openly_and_is_admin() {
    let server = TestServer::start_open().await;
    let device = Device::new().await;
    let relay = server.relay.addr.to_string();
    let pin = server.state.keys.tls_spki_sha256();

    let id = device
        .client
        .register_open(&relay, pin, ADMIN, "")
        .await
        .check("register_open as the admin");
    assert_eq!(id, user(ADMIN));
    assert!(
        device.client.is_admin().check("is_admin"),
        "the configured admin username lands as the admin"
    );
}

// spec: client-runtime/open-registration-in-the-core
#[tokio::test]
async fn open_registration_against_an_invite_server_errors_and_saves_no_account() {
    let server = TestServer::start().await; // invite-only
    let device = Device::new().await;
    let relay = server.relay.addr.to_string();
    let pin = server.state.keys.tls_spki_sha256();

    let result = device.client.register_open(&relay, pin, "alice", "").await;
    assert!(
        matches!(result, Err(ClientError::OpenOnboardingUnavailable)),
        "invite-only server must say open onboarding is unavailable, got {result:?}"
    );
    // No account was saved: the device is still unregistered and can retry or
    // point at another server.
    assert!(
        matches!(device.client.is_admin(), Err(ClientError::NotRegistered)),
        "no account should be saved after a failed open registration"
    );
}
