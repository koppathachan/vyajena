//! account-deletion, end to end against the real server: a member deletes their
//! own account with `Client::delete_account`, which calls `Devices.DeleteAccount`
//! and, on success, wipes the device (store + key store) back to a no-account
//! state; a failed call wipes nothing.

mod support;

use client::ClientError;
use client::keystore::{KeyName, KeyStore};
use support::Check;
use support::device::{self};
use support::server::TestServer;

/// All five key-store keys a registered device holds.
const ACCOUNT_KEYS: [KeyName; 5] = [
    KeyName::IdentityKey,
    KeyName::DeviceAuthKey,
    KeyName::MlsSignatureKey,
    KeyName::StorageKey,
    KeyName::SessionToken,
];

// spec: account-deletion/the-initiating-device-wipes-itself-on-success
#[tokio::test(flavor = "multi_thread")]
async fn deleting_the_account_wipes_the_device() {
    let server = TestServer::start().await;
    let admin = device::admin_with_group(&server).await;
    let bob = device::member_of_group(&admin, "bob").await;
    assert!(
        bob.client.user_id().check("user_id").is_some(),
        "bob is registered before the delete"
    );
    assert!(
        bob.keys
            .get(KeyName::IdentityKey)
            .check("identity key")
            .is_some(),
        "bob holds his identity key before the delete"
    );

    bob.client.delete_account().await.check("delete_account");

    // The device is back to a no-account state: no user, and the identity, auth,
    // MLS signature and session keys are gone from the key store.
    assert!(
        bob.client.user_id().check("user_id").is_none(),
        "the account is wiped locally"
    );
    for name in [
        KeyName::IdentityKey,
        KeyName::DeviceAuthKey,
        KeyName::MlsSignatureKey,
        KeyName::SessionToken,
    ] {
        assert!(
            bob.keys.get(name).check("key store").is_none(),
            "{name:?} was deleted on reset"
        );
    }
}

// spec: account-deletion/the-initiating-device-wipes-itself-on-success
#[tokio::test(flavor = "multi_thread")]
async fn a_failed_delete_wipes_nothing() {
    let server = TestServer::start().await;
    let admin = device::admin_with_group(&server).await;
    let mut bob = device::member_of_group(&admin, "bob").await;

    // Offline: the RPC cannot reach the server, so the call fails before any
    // wipe (account-deletion: No wipe without server confirmation).
    bob.client.stop();
    let result = bob.client.delete_account().await;
    assert!(
        matches!(result, Err(ClientError::Offline)),
        "an offline delete fails: {result:?}"
    );

    // Nothing was wiped: the account and every key-store key are still there.
    assert!(
        bob.client.user_id().check("user_id").is_some(),
        "the account is kept after a failed delete"
    );
    for name in ACCOUNT_KEYS {
        assert!(
            bob.keys.get(name).check("key store").is_some(),
            "{name:?} is kept after a failed delete"
        );
    }

    // The device still works after a restart (the store is intact).
    bob.restart().await;
    assert!(bob.client.user_id().check("user_id").is_some());
}

/// A small picture as a PNG file, as a picker could hand it over.
fn picture() -> Vec<u8> {
    use image::{DynamicImage, ImageFormat, Rgba, RgbaImage};
    let image = RgbaImage::from_fn(64, 48, |x, y| Rgba([x as u8, y as u8, 120, 255]));
    let mut out = Vec::new();
    DynamicImage::ImageRgba8(image)
        .write_to(&mut std::io::Cursor::new(&mut out), ImageFormat::Png)
        .check("png");
    out
}

// spec: account-deletion/the-initiating-device-wipes-itself-on-success
#[tokio::test(flavor = "multi_thread")]
async fn deleting_the_account_clears_the_media_cache_files() {
    let server = TestServer::start().await;
    let admin = device::admin_with_group(&server).await;
    let bob = device::member_of_group(&admin, "bob").await;
    bob.client
        .send_image(&picture(), None, &[])
        .await
        .check("send_image");
    let media = bob.data_dir().join("media");
    let cached = |dir: &std::path::Path| std::fs::read_dir(dir).map(|d| d.count()).unwrap_or(0);
    assert!(cached(&media) > 0, "the picture is cached on disk");

    bob.client.delete_account().await.check("delete_account");

    assert_eq!(cached(&media), 0, "no decrypted media is left on disk");
}
