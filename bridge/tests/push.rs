//! app-ui: Push notifications, the bridge's part: the app's token reaches the
//! Pi through `Core::set_push_token`, a rotated one replaces it, and
//! `clear_push_token` clears it.

mod support;

use bridge::api::error::CoreErrorKind;
use bridge::api::types::PushPlatform;
use support::Check;
use support::device::{self, Device};
use support::pi::TestPi;

async fn stored(pi: &TestPi, device: &Device) -> Option<(i32, String)> {
    let hex = device
        .core
        .device_id()
        .check("device id")
        .check("device id");
    let id: Vec<u8> = (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).check("hex"))
        .collect();
    pi.query(move |db| {
        db.query_row(
            "SELECT push_platform, push_token FROM devices WHERE device_id = ?1",
            [id],
            |row| {
                Ok(row
                    .get::<_, Option<i32>>(0)?
                    .zip(row.get::<_, Option<String>>(1)?))
            },
        )
    })
    .await
}

// spec: app-ui/push-notifications
#[tokio::test(flavor = "multi_thread")]
async fn the_apps_token_reaches_the_pi_and_rotates_and_clears() {
    let pi = TestPi::start().await;
    let admin = device::admin_with_group(&pi).await;
    let apns = "ab".repeat(32);
    admin
        .core
        .set_push_token(PushPlatform::Apns, apns.clone())
        .await
        .check("set");
    assert_eq!(stored(&pi, &admin).await, Some((2, apns)));
    admin
        .core
        .set_push_token(PushPlatform::Fcm, "rotated".into())
        .await
        .check("rotate");
    assert_eq!(stored(&pi, &admin).await, Some((1, "rotated".into())));
    admin.core.clear_push_token().await.check("clear");
    assert_eq!(stored(&pi, &admin).await, None);

    // A token the Pi refuses comes back as an error, not silently.
    let bad = admin
        .core
        .set_push_token(PushPlatform::Apns, "not hex".into())
        .await;
    assert!(bad.is_err());

    // Offline: the app retries on the next connect.
    admin.core.stop();
    let offline = admin
        .core
        .set_push_token(PushPlatform::Fcm, "later".into())
        .await;
    assert_eq!(offline.err().map(|e| e.kind), Some(CoreErrorKind::Offline));
}
