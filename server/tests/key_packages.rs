//! key-directory: KeyPackage upload; KeyPackage fetch (Keys.UploadKeyPackages,
//! Keys.FetchKeyPackages).

mod support;

use openmls::prelude::{Ciphersuite, CredentialWithKey};
use openmls_basic_credential::SignatureKeyPair;
use protocol::device_identity::DeviceId;
use support::clients::{CIPHERSUITE, TestDevice};
use support::harness::{Conn, TestPi, authed};
use support::{Check, bootstrap_admin, invite_user, link_device, pb};
use tonic::Code;

async fn one_time_count(pi: &TestPi, device: &TestDevice) -> u32 {
    let id = device.device_id;
    pi.state
        .store
        .call(move |db| server::store::records::one_time_key_packages(db, &id))
        .await
        .check("count")
}

async fn stored_count(pi: &TestPi) -> i64 {
    pi.query(|db| db.query_row("SELECT count(*) FROM key_packages", [], |r| r.get(0)))
        .await
}

async fn upload(
    conn: &Conn,
    device: &TestDevice,
    key_packages: Vec<Vec<u8>>,
    last_resort: Vec<u8>,
) -> Result<(), tonic::Status> {
    conn.keys()
        .upload_key_packages(authed(
            device.token(),
            pb::UploadKeyPackagesRequest {
                key_packages,
                last_resort_key_package: last_resort,
            },
        ))
        .await
        .map(drop)
}

async fn fetch(
    conn: &Conn,
    caller: &TestDevice,
    user: pb::UserId,
    device_ids: &[DeviceId],
) -> Result<pb::FetchKeyPackagesResponse, tonic::Status> {
    Ok(conn
        .keys()
        .fetch_key_packages(authed(
            caller.token(),
            pb::FetchKeyPackagesRequest {
                user_id: Some(user),
                device_ids: device_ids.iter().map(|id| id.to_vec()).collect(),
            },
        ))
        .await?
        .into_inner())
}

// spec: key-directory/keypackage-upload
#[tokio::test]
async fn twenty_one_time_and_one_last_resort_are_stored() {
    let pi = TestPi::start().await;
    let conn = pi.connect().await;
    let (_, admin) = bootstrap_admin(&pi, &conn).await;
    admin.upload_key_packages(&conn, 20).await;
    assert_eq!(one_time_count(&pi, &admin).await, 20);
    assert_eq!(stored_count(&pi).await, 21);

    // A second upload adds one-time KeyPackages and replaces the last resort.
    admin.upload_key_packages(&conn, 2).await;
    assert_eq!(one_time_count(&pi, &admin).await, 22);
    assert_eq!(stored_count(&pi).await, 23);
}

// spec: key-directory/keypackage-upload
#[tokio::test]
async fn keypackage_for_another_device_fails_the_whole_upload() {
    let pi = TestPi::start().await;
    let conn = pi.connect().await;
    let (_, admin) = bootstrap_admin(&pi, &conn).await;
    let (alice, laptop) = invite_user(&conn, &admin, "alice").await;
    let phone = link_device(&conn, &alice, &laptop).await;

    // The phone signs a KeyPackage whose credential is the laptop's certificate.
    let foreign = phone.key_package_with(CredentialWithKey {
        credential: laptop.credential_with_key().credential,
        signature_key: phone.signer.public().into(),
    });
    let result = upload(
        &conn,
        &phone,
        vec![phone.key_package(), foreign, phone.key_package()],
        phone.key_package(),
    )
    .await;
    assert_eq!(result.err().map(|s| s.code()), Some(Code::PermissionDenied));

    // Another user's certificate is just as foreign.
    let (_, bob) = invite_user(&conn, &admin, "bob").await;
    let bobs = phone.key_package_with(CredentialWithKey {
        credential: bob.credential_with_key().credential,
        signature_key: phone.signer.public().into(),
    });
    let result = upload(&conn, &phone, vec![], bobs).await;
    assert_eq!(result.err().map(|s| s.code()), Some(Code::PermissionDenied));
    assert_eq!(
        stored_count(&pi).await,
        0,
        "nothing of a failed upload is kept"
    );
}

// spec: key-directory/keypackage-upload
#[tokio::test]
async fn signature_key_must_be_the_certificates_mls_key() {
    let pi = TestPi::start().await;
    let conn = pi.connect().await;
    let (_, admin) = bootstrap_admin(&pi, &conn).await;
    let other = SignatureKeyPair::new(CIPHERSUITE.signature_algorithm()).check("key");
    // The device's genuine certificate, paired with another signature key.
    let swapped = admin.key_package_signed_by(
        CIPHERSUITE,
        &other,
        CredentialWithKey {
            credential: admin.credential_with_key().credential,
            signature_key: other.public().into(),
        },
    );
    let result = upload(&conn, &admin, vec![swapped], admin.key_package()).await;
    assert_eq!(result.err().map(|s| s.code()), Some(Code::PermissionDenied));
    assert_eq!(stored_count(&pi).await, 0);
}

// spec: key-directory/keypackage-upload
#[tokio::test]
async fn unparsable_wrong_suite_or_missing_last_resort_is_invalid() {
    let pi = TestPi::start().await;
    let conn = pi.connect().await;
    let (_, admin) = bootstrap_admin(&pi, &conn).await;

    let garbage = upload(
        &conn,
        &admin,
        vec![b"not a KeyPackage".to_vec()],
        admin.key_package(),
    )
    .await;
    assert_eq!(garbage.err().map(|s| s.code()), Some(Code::InvalidArgument));

    let mut truncated = admin.key_package();
    truncated.pop();
    let truncated = upload(&conn, &admin, vec![truncated], admin.key_package()).await;
    assert_eq!(
        truncated.err().map(|s| s.code()),
        Some(Code::InvalidArgument)
    );

    let p256 = Ciphersuite::MLS_128_DHKEMP256_AES128GCM_SHA256_P256;
    let p256_key = SignatureKeyPair::new(p256.signature_algorithm()).check("P-256 key");
    let wrong_suite = admin.key_package_signed_by(
        p256,
        &p256_key,
        CredentialWithKey {
            credential: admin.credential_with_key().credential,
            signature_key: p256_key.public().into(),
        },
    );
    let wrong_suite = upload(&conn, &admin, vec![wrong_suite], admin.key_package()).await;
    assert_eq!(
        wrong_suite.err().map(|s| s.code()),
        Some(Code::InvalidArgument)
    );

    let no_last_resort = upload(&conn, &admin, vec![admin.key_package()], vec![]).await;
    assert_eq!(
        no_last_resort.err().map(|s| s.code()),
        Some(Code::InvalidArgument)
    );
    assert_eq!(stored_count(&pi).await, 0);
}

// spec: key-directory/keypackage-upload
#[tokio::test]
async fn keypackage_without_the_admin_extension_capability_fails_the_whole_upload() {
    let pi = TestPi::start().await;
    let conn = pi.connect().await;
    let (_, admin) = bootstrap_admin(&pi, &conn).await;
    let (_, phone) = invite_user(&conn, &admin, "bob").await;

    let lacking = phone.key_package_without_admin_capability();
    let one_time = upload(
        &conn,
        &phone,
        vec![phone.key_package(), lacking.clone()],
        phone.key_package(),
    )
    .await;
    assert_eq!(
        one_time.err().map(|s| s.code()),
        Some(Code::InvalidArgument)
    );
    let last_resort = upload(&conn, &phone, vec![phone.key_package()], lacking).await;
    assert_eq!(
        last_resort.err().map(|s| s.code()),
        Some(Code::InvalidArgument)
    );
    assert_eq!(stored_count(&pi).await, 0, "the Pi stores nothing");
}

// spec: key-directory/keypackage-fetch
#[tokio::test]
async fn two_callers_get_different_one_time_keypackages() {
    let pi = TestPi::start().await;
    let conn = pi.connect().await;
    let (_, admin) = bootstrap_admin(&pi, &conn).await;
    let (bob_user, bob) = invite_user(&conn, &admin, "bob").await;
    let (_, carol) = invite_user(&conn, &admin, "carol").await;
    bob.upload_key_packages(&conn, 2).await;

    let first = fetch(&conn, &admin, bob_user.pb_user_id(), &[])
        .await
        .check("fetch 1");
    let second = fetch(&conn, &carol, bob_user.pb_user_id(), &[])
        .await
        .check("fetch 2");
    assert_eq!(first.identity_public_key, bob_user.identity_public_key());
    assert_eq!(first.devices.len(), 1);
    assert_eq!(second.devices.len(), 1);
    assert_eq!(first.devices[0].certificate, Some(bob.certificate.clone()));
    assert_ne!(first.devices[0].key_package, second.devices[0].key_package);
    assert_eq!(one_time_count(&pi, &bob).await, 0);
}

// spec: key-directory/keypackage-fetch
#[tokio::test]
async fn exhausted_device_falls_back_to_its_last_resort_keypackage() {
    let pi = TestPi::start().await;
    let conn = pi.connect().await;
    let (_, admin) = bootstrap_admin(&pi, &conn).await;
    let (bob_user, bob) = invite_user(&conn, &admin, "bob").await;
    let one_time = bob.key_package();
    let last_resort = bob.key_package();
    upload(&conn, &bob, vec![one_time.clone()], last_resort.clone())
        .await
        .check("upload");

    let got = |n: &str| {
        let conn = conn.clone();
        let user = bob_user.pb_user_id();
        let token = admin.token().to_owned();
        let what = n.to_owned();
        async move {
            conn.keys()
                .fetch_key_packages(authed(
                    &token,
                    pb::FetchKeyPackagesRequest {
                        user_id: Some(user),
                        device_ids: vec![],
                    },
                ))
                .await
                .check(&what)
                .into_inner()
                .devices
                .into_iter()
                .map(|d| d.key_package)
                .collect::<Vec<_>>()
        }
    };
    assert_eq!(got("first").await, vec![one_time]);
    assert_eq!(got("second").await, vec![last_resort.clone()]);
    assert_eq!(
        got("third").await,
        vec![last_resort],
        "the last resort stays"
    );
}

// spec: key-directory/keypackage-fetch
#[tokio::test]
async fn fetch_for_one_device_consumes_only_that_device() {
    let pi = TestPi::start().await;
    let conn = pi.connect().await;
    let (_, admin) = bootstrap_admin(&pi, &conn).await;
    let (bob_user, tablet) = invite_user(&conn, &admin, "bob").await;
    let phone = link_device(&conn, &bob_user, &tablet).await;
    tablet.upload_key_packages(&conn, 3).await;
    phone.upload_key_packages(&conn, 3).await;

    let response = fetch(&conn, &admin, bob_user.pb_user_id(), &[phone.device_id])
        .await
        .check("fetch phone only");
    assert_eq!(response.devices.len(), 1);
    assert_eq!(
        response.devices[0].certificate,
        Some(phone.certificate.clone())
    );
    assert_eq!(one_time_count(&pi, &phone).await, 2);
    assert_eq!(
        one_time_count(&pi, &tablet).await,
        3,
        "the tablet keeps all of them"
    );

    // Without a list: one for each device.
    let all = fetch(&conn, &admin, bob_user.pb_user_id(), &[])
        .await
        .check("fetch all");
    assert_eq!(all.devices.len(), 2);
    assert_eq!(one_time_count(&pi, &tablet).await, 2);
}

// spec: key-directory/keypackage-fetch
#[tokio::test]
async fn unknown_user_is_not_found() {
    let pi = TestPi::start().await;
    let conn = pi.connect().await;
    let (_, admin) = bootstrap_admin(&pi, &conn).await;
    let nobody = pb::UserId {
        username: "nobody".to_owned(),
        server_id: "pi".to_owned(),
    };
    let result = fetch(&conn, &admin, nobody, &[]).await;
    assert_eq!(result.err().map(|s| s.code()), Some(Code::NotFound));
    let elsewhere = pb::UserId {
        username: "akhil".to_owned(),
        server_id: "other-pi".to_owned(),
    };
    let result = fetch(&conn, &admin, elsewhere, &[]).await;
    assert_eq!(result.err().map(|s| s.code()), Some(Code::NotFound));
}

// spec: key-directory/keypackage-fetch
#[tokio::test]
async fn listing_a_foreign_or_revoked_device_is_not_found_and_consumes_nothing() {
    let pi = TestPi::start().await;
    let conn = pi.connect().await;
    let (_, admin) = bootstrap_admin(&pi, &conn).await;
    let (bob_user, tablet) = invite_user(&conn, &admin, "bob").await;
    let phone = link_device(&conn, &bob_user, &tablet).await;
    let (_, carol) = invite_user(&conn, &admin, "carol").await;
    tablet.upload_key_packages(&conn, 2).await;
    phone.upload_key_packages(&conn, 2).await;

    // Carol's device is not Bob's.
    let result = fetch(
        &conn,
        &admin,
        bob_user.pb_user_id(),
        &[tablet.device_id, carol.device_id],
    )
    .await;
    assert_eq!(result.err().map(|s| s.code()), Some(Code::NotFound));
    assert_eq!(one_time_count(&pi, &tablet).await, 2, "nothing consumed");

    // A revoked device is not listed either, with or without a list.
    conn.devices()
        .revoke_device(authed(
            tablet.token(),
            pb::RevokeDeviceRequest {
                device_id: phone.device_id.to_vec(),
            },
        ))
        .await
        .check("RevokeDevice");
    let result = fetch(&conn, &admin, bob_user.pb_user_id(), &[phone.device_id]).await;
    assert_eq!(result.err().map(|s| s.code()), Some(Code::NotFound));
    let all = fetch(&conn, &admin, bob_user.pb_user_id(), &[])
        .await
        .check("fetch all");
    assert_eq!(all.devices.len(), 1);
    assert_eq!(all.devices[0].certificate, Some(tablet.certificate.clone()));
}
