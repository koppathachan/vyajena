//! What the app's own-message look and notices need from the bridge
//! (qa-fixes task 3.1): `own_user` on messages, `MembersChanged` with the
//! devices and the committer, `GroupSettingsChanged` with the previous name
//! and who renamed, `Removed`, and `reconnect_now`.

mod support;

use std::time::Duration;

use bridge::api::types::{ChatBody, ConnectionStatus, CoreEvent, MemberDevice, SendState};
use support::Check;
use support::device::{self, Device, user};
use support::server::{ADMIN, TestServer};

async fn wait_accepted(device: &mut Device, id: i64) {
    device
        .wait_event("accepted", |e| {
            matches!(e, CoreEvent::SendStatus { id: i, status: SendState::Accepted } if *i == id)
        })
        .await;
}

fn member_of(device: &Device, username: &str) -> MemberDevice {
    MemberDevice {
        user: user(username),
        device: device
            .core
            .device_id()
            .check("device id")
            .check("registered"),
    }
}

// spec: app-ui/chat-screen
#[tokio::test(flavor = "multi_thread")]
async fn own_user_marks_messages_of_this_devices_user() {
    let server = TestServer::start().await;
    let admin = device::admin_with_group(&server).await;
    let mut alice = device::member_of_group(&admin, "alice").await;
    let mut bob = device::member_of_group(&admin, "bob").await;
    alice
        .wait_until("alice sees bob", |c| {
            c.members()
                .is_ok_and(|m| m.iter().any(|d| d.user == user("bob")))
        })
        .await;

    let id = alice.core.send_text("hi".to_owned()).check("send");
    wait_accepted(&mut alice, id).await;
    let own = alice.core.message(id).check("message").check("stored");
    assert!(own.own_user && own.outgoing);
    let received = bob
        .wait_event("hi", |e| matches!(e, CoreEvent::Message { .. }))
        .await;
    let CoreEvent::Message { message } = received else {
        unreachable!()
    };
    assert_eq!(message.body, ChatBody::Text { text: "hi".into() });
    assert!(!message.own_user && !message.outgoing);
}

// spec: client-runtime/events-in-order
#[tokio::test(flavor = "multi_thread")]
async fn members_changed_and_renames_name_what_changed_and_who() {
    let server = TestServer::start().await;
    let mut admin = device::admin_with_group(&server).await;
    let mut bob = device::member_of_group(&admin, "bob").await;
    let bob_member = member_of(&bob, "bob");

    let added = admin
        .wait_event(
            "bob added",
            |e| matches!(e, CoreEvent::MembersChanged { added, .. } if !added.is_empty()),
        )
        .await;
    assert_eq!(
        added,
        CoreEvent::MembersChanged {
            added: vec![bob_member.clone()],
            removed: Vec::new(),
            by: Some(user(ADMIN)),
            automatic: Vec::new(),
        }
    );
    // Bob's own join: no devices, no committer, so no notice.
    bob.wait_event("joined", |e| {
        *e == CoreEvent::MembersChanged {
            added: Vec::new(),
            removed: Vec::new(),
            by: None,
            automatic: Vec::new(),
        }
    })
    .await;

    // First name: nothing stored before, so `previous` is unset.
    let id = admin
        .core
        .set_group_name(Some("Home chat".into()))
        .check("rename");
    wait_accepted(&mut admin, id).await;
    bob.wait_event("first name", |e| {
        *e == CoreEvent::GroupSettingsChanged {
            name: Some("Home chat".into()),
            previous: None,
            by: user(ADMIN),
        }
    })
    .await;
    let id = admin
        .core
        .set_group_name(Some("Family".into()))
        .check("rename");
    wait_accepted(&mut admin, id).await;
    bob.wait_event("rename", |e| {
        *e == CoreEvent::GroupSettingsChanged {
            name: Some("Family".into()),
            previous: Some("Home chat".into()),
            by: user(ADMIN),
        }
    })
    .await;

    // The admin removes Bob: the admin sees him removed by the admin, and
    // Bob's core says this device was removed and stops; history stays.
    let before = bob.core.history(10).check("history").len();
    admin
        .core
        .remove_user(user("bob"))
        .await
        .check("remove bob");
    let removed = admin
        .wait_event(
            "bob removed",
            |e| matches!(e, CoreEvent::MembersChanged { removed, .. } if !removed.is_empty()),
        )
        .await;
    assert_eq!(
        removed,
        CoreEvent::MembersChanged {
            added: Vec::new(),
            removed: vec![bob_member],
            by: Some(user(ADMIN)),
            automatic: Vec::new(),
        }
    );
    bob.wait_event("removed", |e| matches!(e, CoreEvent::Removed))
        .await;
    assert!(!bob.core.is_connected());
    assert_eq!(bob.core.history(10).check("history").len(), before);
    // reconnect_now does not bring a removed device back.
    bob.core.reconnect_now();
    tokio::time::sleep(Duration::from_millis(800)).await;
    assert!(!bob.core.is_connected());
    let after_removed: Vec<CoreEvent> = bob
        .events()
        .into_iter()
        .skip_while(|e| !matches!(e, CoreEvent::Removed))
        .skip(1)
        .collect();
    assert!(
        !after_removed.iter().any(|e| matches!(
            e,
            CoreEvent::Removed
                | CoreEvent::Connection {
                    status: ConnectionStatus::Connecting | ConnectionStatus::Connected
                }
        )),
        "{after_removed:?}"
    );
}

// spec: app-ui/foreground-and-background
#[tokio::test(flavor = "multi_thread")]
async fn reconnect_now_while_connected_does_nothing() {
    let server = TestServer::start().await;
    let admin = device::admin_with_group(&server).await;
    let seen = admin.events().len();
    admin.core.reconnect_now();
    tokio::time::sleep(Duration::from_millis(800)).await;
    assert!(admin.core.is_connected());
    assert!(
        !admin.events()[seen..].iter().any(|e| matches!(
            e,
            CoreEvent::Connection {
                status: ConnectionStatus::Connecting
            }
        )),
        "{:?}",
        admin.events()
    );
}
