//! The host side of the app's integration test (flutter-app task 6.2,
//! design D6/D7): the admin's device, driven through the bridge's `Core`
//! against the temp Pi that `app/integration_test/run.sh` starts. It is
//! ignored by `cargo test`; the script runs it with `--ignored` while the
//! app runs on a simulator or emulator.
//!
//! What it does, with the app (Bob) on the other side:
//!
//! 1. registers the admin from the Pi's bootstrap invite
//!    (`VYAJENA_PEER_BOOTSTRAP`), creates the group and an invite for
//!    `bob`, and writes its text to `$VYAJENA_PEER_DIR/invite.txt`;
//! 2. stays connected and does nothing: the Pi's `JoinNotice` makes this
//!    device add Bob's phone on its own (group-membership: Automatic add of
//!    new users);
//! 3. waits for Bob's "hi", replies "hey" and waits until the Pi accepted
//!    it, then writes `done`.

mod support;

use std::time::Duration;

use bridge::api::types::{ChatBody, CoreEvent, SendState};
use support::Check;
use support::device::{Device, user};
use support::pi::ADMIN;

/// How long the peer waits for the app: building and installing the app
/// on a simulator takes minutes.
const APP_WAIT: Duration = Duration::from_secs(15 * 60);

fn env(name: &str) -> String {
    std::env::var(name)
        .unwrap_or_else(|_| panic!("{name} is not set (run app/integration_test/run.sh)"))
}

/// Writes `text` to `path` in one step (the script polls for the file).
fn publish(path: &std::path::Path, text: &str) {
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, text).check("write");
    std::fs::rename(&tmp, path).check("rename");
}

// spec: group-membership/automatic-add-of-new-users
// spec: app-ui/chat-screen
#[tokio::test(flavor = "multi_thread")]
#[ignore = "the host side of app/integration_test; run by app/integration_test/run.sh"]
async fn app_peer() {
    support::init_logging();
    let bootstrap = env("VYAJENA_PEER_BOOTSTRAP");
    let out = std::path::PathBuf::from(env("VYAJENA_PEER_DIR"));

    let mut admin = Device::on_system_clock().await;
    let id = admin.core.register(bootstrap).await.check("register admin");
    assert_eq!(id, user(ADMIN));
    admin.core.start().await.check("start admin");
    admin.wait_connected().await;
    admin.core.create_group().await.check("create group");
    let invite = admin
        .core
        .create_invite("bob".to_owned())
        .await
        .check("create invite");
    publish(&out.join("invite.txt"), &invite.text.check("invite text"));
    eprintln!("app_peer: invite for bob written; waiting for the app");

    // No add_user: the admin's device adds Bob's phone on the JoinNotice.
    admin
        .wait_until_for("bob's phone added", APP_WAIT, |c| {
            c.members()
                .is_ok_and(|m| m.iter().any(|d| d.user == user("bob")))
        })
        .await;
    eprintln!("app_peer: bob's phone is in the group");

    admin
        .wait_event_for("bob's \"hi\"", APP_WAIT, |e| {
            matches!(e, CoreEvent::Message { message }
                if message.sender == user("bob")
                    && message.body == ChatBody::Text { text: "hi".to_owned() })
        })
        .await;
    let hey = admin.core.send_text("hey".to_owned()).check("send hey");
    admin
        .wait_event("hey accepted", |e| {
            matches!(e, CoreEvent::SendStatus { id, status: SendState::Accepted } if *id == hey)
        })
        .await;
    eprintln!("app_peer: replied \"hey\"");
    publish(&out.join("done"), "done");
}
