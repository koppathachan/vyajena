//! device-identity on the device, end to end against the real Pi: signup,
//! keys in the key store, linking, revocation, push tokens, invites
//! (client-core task 4.1).

mod support;

use client::identity::payload;
use client::keystore::{KeyName, KeyStore};
use client::{ClientError, Event, PushPlatform, PushToken};
use support::Check;
use support::device::{self, Device, member, user};
use support::pi::{ADMIN, TestPi};

// spec: device-identity/signup-with-an-invite
// spec: device-identity/user-identity-key-and-device-certificates
// spec: device-identity/admin-owned-usernames
#[tokio::test(flavor = "multi_thread")]
async fn a_first_device_signs_up_with_an_invite() {
    let pi = TestPi::start().await;
    let admin = device::admin(&pi).await;
    let invite = admin.client.create_invite("alice").await.check("invite");
    assert_eq!(invite.user, user("alice"));

    let alice = Device::new().await;
    // The text form the admin would share works as well as the bytes.
    let text = payload::to_text(&invite.payload);
    let bytes = payload::from_text(&text).check("invite text");
    let id = alice.client.register(&bytes).await.check("register");
    assert_eq!(id, user("alice"));
    assert_eq!(alice.user(), user("alice"));

    // The Pi holds her identity key and certificate.
    let identity = alice
        .keys
        .get(KeyName::IdentityKey)
        .check("get")
        .check("identity key");
    let alice_id = alice.device_id();
    let (stored_identity, uploaded): (Vec<u8>, i64) = pi
        .query(move |conn| {
            conn.query_row(
                "SELECT u.identity_public_key,
                        (SELECT count(*) FROM key_packages k WHERE k.device_id = d.device_id)
                 FROM devices d JOIN users u ON u.username = d.username
                 WHERE d.device_id = ?1",
                [alice_id.as_slice()],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
        })
        .await;
    let public =
        ed25519_dalek::SigningKey::from_bytes(identity.as_slice().try_into().check("32-byte seed"))
            .verifying_key()
            .to_bytes();
    assert_eq!(stored_identity, public.to_vec());
    // KeyPackages were uploaded: 20 one-time and one last-resort.
    assert_eq!(uploaded, 21);

    // The invite is used up; a second device cannot register with it.
    let again = Device::new().await;
    assert!(again.client.register(&bytes).await.is_err());
    // A registered device does not register again.
    assert!(matches!(
        alice.client.register(&bytes).await,
        Err(ClientError::AlreadyRegistered)
    ));
}

// spec: client-runtime/key-storage
#[tokio::test(flavor = "multi_thread")]
async fn signup_keeps_the_keys_in_the_key_store_only() {
    let pi = TestPi::start().await;
    let admin = device::admin(&pi).await;
    for name in [
        KeyName::IdentityKey,
        KeyName::DeviceAuthKey,
        KeyName::MlsSignatureKey,
        KeyName::StorageKey,
    ] {
        assert!(
            admin.keys.get(name).check("get").is_some(),
            "{name} missing"
        );
    }
    admin.client.create_group().await.check("create group");
    // No key appears in the database file.
    let db = support::database_bytes(&admin_database(&admin));
    for name in [
        KeyName::IdentityKey,
        KeyName::DeviceAuthKey,
        KeyName::MlsSignatureKey,
        KeyName::StorageKey,
    ] {
        let secret = admin.keys.get(name).check("get").check("key");
        assert!(
            !support::contains(&db, &secret),
            "{name} is in the database"
        );
    }
}

fn admin_database(device: &Device) -> std::path::PathBuf {
    device.data_dir().join(client::DATABASE_FILE)
}

// spec: device-identity/device-linking
// spec: device-identity/user-identity-key-and-device-certificates
#[tokio::test(flavor = "multi_thread")]
async fn a_new_device_is_linked_and_added_by_the_identity_device() {
    let pi = TestPi::start().await;
    let admin = device::admin_with_group(&pi).await;

    let mut laptop = Device::new().await;
    let request = laptop.client.link_request().await.check("link request");
    let approval = admin.client.approve_link(&request).await.check("approve");
    let id = laptop
        .client
        .complete_link(&approval)
        .await
        .check("complete");
    assert_eq!(id, user(ADMIN));
    // The linked device never holds the identity key and cannot link others.
    assert!(laptop.keys.get(KeyName::IdentityKey).check("get").is_none());
    assert!(laptop.client.is_admin().check("is admin"));
    laptop.client.start().await.check("start laptop");
    laptop
        .wait_event("the laptop joined", |e| matches!(e, Event::JoinedGroup))
        .await;
    let both = [member(&admin), member(&laptop)];
    admin.wait_members(&both).await;
    laptop.wait_members(&both).await;

    let other = Device::new().await;
    let request = other.client.link_request().await.check("link request");
    assert!(matches!(
        laptop.client.approve_link(&request).await,
        Err(ClientError::NotIdentityDevice)
    ));
}

// spec: device-identity/federation-ready-identifiers
#[tokio::test(flavor = "multi_thread")]
async fn a_device_computes_its_own_device_id_without_the_pi() {
    use sha2::Digest as _;

    // No Pi at all: the new device creates its keys and names itself. The
    // link request carries the id, and decoding it checks that the id is
    // derived from the auth key.
    let laptop = Device::new().await;
    let request = laptop.client.link_request().await.check("link request");
    let request = payload::LinkRequest::decode(&request).check("decode");
    let digest = sha2::Sha256::digest(request.auth_public_key);
    assert_eq!(request.device_id().as_slice(), &digest[..16]);
    // The auth key is the one in the key store.
    let auth = laptop
        .keys
        .get(KeyName::DeviceAuthKey)
        .check("get")
        .check("auth key");
    let public =
        ed25519_dalek::SigningKey::from_bytes(auth.as_slice().try_into().check("32-byte seed"))
            .verifying_key()
            .to_bytes();
    assert_eq!(request.auth_public_key, public);
}

// spec: device-identity/device-revocation
#[tokio::test(flavor = "multi_thread")]
async fn a_revoked_device_is_removed_and_cut_off() {
    let pi = TestPi::start().await;
    let admin = device::admin_with_group(&pi).await;
    let mut phone = Device::new().await;
    let request = phone.client.link_request().await.check("link request");
    let approval = admin.client.approve_link(&request).await.check("approve");
    phone
        .client
        .complete_link(&approval)
        .await
        .check("complete");
    phone.client.start().await.check("start");
    phone
        .wait_event("joined", |e| matches!(e, Event::JoinedGroup))
        .await;
    let phone_id = phone.device_id();

    admin.client.revoke_device(&phone_id).await.check("revoke");
    admin.wait_members(&[member(&admin)]).await;
    phone
        .wait_until("the phone's stream to close", |c| !c.is_connected())
        .await;
    // It cannot authenticate again.
    assert!(
        !phone
            .sees_within_quiet(|e| matches!(
                e,
                Event::Connection(client::ConnectionState::Connected)
            ))
            .await
    );
    assert!(!pi.is_connected(&phone_id));
}

// spec: device-identity/push-token-registration
#[tokio::test(flavor = "multi_thread")]
async fn a_device_sets_and_clears_its_push_token() {
    let pi = TestPi::start().await;
    let admin = device::admin(&pi).await;
    let id = admin.device_id();
    let token = |pi: &TestPi| {
        let state = pi.state.clone();
        async move {
            state
                .store
                .call(move |conn| server::store::records::push_token(conn, &id))
                .await
                .check("push token")
        }
    };
    admin
        .client
        .set_push_token(Some(PushToken {
            platform: PushPlatform::Fcm,
            token: "fcm-1".into(),
        }))
        .await
        .check("set");
    assert_eq!(token(&pi).await.check("token").token, "fcm-1");
    admin.client.set_push_token(None).await.check("clear");
    assert!(token(&pi).await.is_none());
}

// spec: device-identity/invites
#[tokio::test(flavor = "multi_thread")]
async fn the_admin_lists_and_revokes_invites_and_others_cannot_invite() {
    let pi = TestPi::start().await;
    let admin = device::admin_with_group(&pi).await;
    let invite = admin.client.create_invite("carol").await.check("invite");
    let listed = admin.client.list_invites().await.check("list");
    assert!(listed.iter().any(|i| i.code == invite.code));
    admin
        .client
        .revoke_invite(&invite.code)
        .await
        .check("revoke");
    let listed = admin.client.list_invites().await.check("list");
    assert!(listed.iter().all(|i| i.code != invite.code));
    let carol = Device::new().await;
    assert!(carol.client.register(&invite.payload).await.is_err());

    let alice = device::member_of_group(&admin, "alice").await;
    assert!(alice.client.create_invite("mallory").await.is_err());
}
