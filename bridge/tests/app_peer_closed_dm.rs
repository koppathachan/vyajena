//! The host side of the app's closed-DM removal integration test (change
//! `remove-closed-dm`): the admin's device and Carol's device, driven through the
//! bridge's `Core` against the temp server that
//! `app/integration_test/run.sh closeddm` starts. Ignored by `cargo test`.
//!
//! With the app (Bob) on the other side it:
//!
//! 1. registers the admin, creates the group, publishes Bob's invite;
//! 2. waits for Bob's phone, adds Carol, who opens a DM with Bob and says "bye";
//! 3. waits for Bob's "ready" in the lobby (he has the DM and its message), then
//!    removes Carol from the server, which closes the DM on Bob's phone;
//! 4. waits for the app's `done` marker folder entry (the app removed the DM).

mod support;

use std::time::Duration;

use bridge::api::types::{ChatBody, CoreEvent, SendState};
use support::Check;
use support::device::{Device, member_of_group, user};
use support::server::ADMIN;

const APP_WAIT: Duration = Duration::from_secs(15 * 60);

fn env(name: &str) -> String {
    std::env::var(name)
        .unwrap_or_else(|_| panic!("{name} is not set (run app/integration_test/run.sh)"))
}

fn publish(path: &std::path::Path, text: &str) {
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, text).check("write");
    std::fs::rename(&tmp, path).check("rename");
}

// spec: message-deletion/remove-a-closed-dm
// spec: app-ui/remove-a-closed-dm-from-the-list
#[tokio::test(flavor = "multi_thread")]
#[ignore = "the host side of app/integration_test; run by app/integration_test/run.sh"]
async fn app_peer_closed_dm() {
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

    admin
        .wait_until_for("bob's phone added", APP_WAIT, |c| {
            c.members()
                .is_ok_and(|m| m.iter().any(|d| d.user == user("bob")))
        })
        .await;
    let mut carol = member_of_group(&admin, "carol").await;
    carol
        .wait_until("carol sees bob", |c| {
            c.members()
                .is_ok_and(|m| m.iter().any(|d| d.user == user("bob")))
        })
        .await;
    let chat = carol.core.open_dm(user("bob")).await.check("open DM");
    let bye = carol
        .core
        .send_message_in(chat, "bye".to_owned(), vec![], None)
        .check("send bye");
    carol
        .wait_event_for("bye accepted", APP_WAIT, |e| {
            matches!(e, CoreEvent::SendStatus { id, status: SendState::Accepted } if *id == bye)
        })
        .await;
    eprintln!("app_peer: carol said bye in the DM; waiting for bob's ready");

    admin
        .wait_event_for("bob's \"ready\"", APP_WAIT, |e| {
            matches!(e, CoreEvent::Message { message }
                if message.sender == user("bob")
                    && message.body == ChatBody::Text { text: "ready".to_owned() })
        })
        .await;
    admin
        .core
        .remove_user(user("carol"))
        .await
        .check("remove carol");
    eprintln!("app_peer: carol removed; the DM is closed on bob's phone");
    publish(&out.join("closed"), "closed");

    // The app removes the DM, then writes nothing: it ends. Stay connected
    // until the script stops us.
    tokio::time::sleep(APP_WAIT).await;
}
