//! notification-prefs end to end against the real server (change
//! `mute-notifications`): muting and unmuting a conversation, whole-app mute and
//! its override, and the mute set a second device adopts on connect.
//!
//! The in-app notification/badge suppression itself is the app's job (app-ui:
//! Mute a conversation; widget tests); the core keeps the mute set, exposes it,
//! and marks muted chats, which these tests cover.

mod support;

use client::{ChatId, Event, MuteTarget};
use support::Check;
use support::device::{self, Device, member};
use support::server::TestServer;

/// The admin, Alice and Bob, all in the group and seeing each other.
async fn group(server: &TestServer) -> (Device, Device, Device) {
    let admin = device::admin_with_group(server).await;
    let alice = device::member_of_group(&admin, "alice").await;
    let bob = device::member_of_group(&admin, "bob").await;
    let everyone = [member(&admin), member(&alice), member(&bob)];
    for device in [&admin, &alice, &bob] {
        device.wait_members(&everyone).await;
    }
    (admin, alice, bob)
}

/// Whether `device` marks the lobby row muted in its chat list.
fn lobby_row_muted(device: &Device) -> bool {
    device
        .client
        .chats()
        .check("chats")
        .into_iter()
        .find(|c| c.id == ChatId::MAIN)
        .is_some_and(|c| c.muted)
}

// spec: notification-prefs/mute-a-conversation
#[tokio::test(flavor = "multi_thread")]
async fn alice_mutes_and_unmutes_the_lobby() {
    let server = TestServer::start().await;
    let (_admin, mut alice, bob) = group(&server).await;

    assert!(!alice.client.is_muted(ChatId::MAIN).check("muted?"));
    assert!(!lobby_row_muted(&alice));

    alice
        .client
        .set_mute_pref(MuteTarget::Conversation(ChatId::MAIN), true)
        .await
        .check("mute the lobby");
    alice
        .wait_event("the mute set changed", |e| {
            matches!(e, Event::MutePrefsChanged)
        })
        .await;

    assert!(alice.client.is_muted(ChatId::MAIN).check("muted?"));
    assert!(lobby_row_muted(&alice));
    // Per user: Bob's view is unchanged.
    assert!(!bob.client.is_muted(ChatId::MAIN).check("bob muted?"));

    alice
        .client
        .set_mute_pref(MuteTarget::Conversation(ChatId::MAIN), false)
        .await
        .check("unmute the lobby");
    assert!(!alice.client.is_muted(ChatId::MAIN).check("muted?"));
    assert!(!lobby_row_muted(&alice));
}

// spec: notification-prefs/mute-the-whole-app
#[tokio::test(flavor = "multi_thread")]
async fn whole_app_mute_overrides_the_per_conversation_set() {
    let server = TestServer::start().await;
    let (_admin, mut alice, _bob) = group(&server).await;

    alice
        .client
        .set_mute_pref(MuteTarget::Global, true)
        .await
        .check("mute all");
    alice
        .wait_event("the mute set changed", |e| {
            matches!(e, Event::MutePrefsChanged)
        })
        .await;

    // Everything is silenced, though no conversation was muted individually.
    let prefs = alice.client.mute_prefs().check("prefs");
    assert!(prefs.muted_all);
    assert!(prefs.muted_chats.is_empty());
    assert!(alice.client.is_muted(ChatId::MAIN).check("muted?"));
    // The row is not itself marked muted: mute-all is a global state.
    assert!(!lobby_row_muted(&alice));

    alice
        .client
        .set_mute_pref(MuteTarget::Global, false)
        .await
        .check("clear mute all");
    assert!(!alice.client.is_muted(ChatId::MAIN).check("muted?"));
}

// spec: notification-prefs/mute-preference-sync
#[tokio::test(flavor = "multi_thread")]
async fn a_second_device_adopts_the_mute_set_on_connect() {
    let server = TestServer::start().await;
    let (_admin, alice, _bob) = group(&server).await;

    // Alice mutes the lobby on her phone.
    alice
        .client
        .set_mute_pref(MuteTarget::Conversation(ChatId::MAIN), true)
        .await
        .check("mute the lobby");

    // She links a tablet, which joins the group.
    let mut tablet = Device::new().await;
    let request = tablet.client.link_request().await.check("link request");
    let approval = alice.client.approve_link(&request).await.check("approve");
    tablet
        .client
        .complete_link(&approval)
        .await
        .check("complete");
    tablet.client.start().await.check("start tablet");
    tablet
        .wait_event("the tablet joined", |e| matches!(e, Event::JoinedGroup))
        .await;

    // On connect the tablet adopts Alice's mute set wholesale.
    tablet
        .wait_until("the lobby muted on the tablet", |c| {
            c.is_muted(ChatId::MAIN).unwrap_or(false)
        })
        .await;
    assert!(
        tablet
            .client
            .mute_prefs()
            .check("prefs")
            .muted_chats
            .contains(&ChatId::MAIN)
    );

    // Unmuting on the phone reaches the tablet on its next connect.
    alice
        .client
        .set_mute_pref(MuteTarget::Conversation(ChatId::MAIN), false)
        .await
        .check("unmute the lobby");
    tablet.restart().await;
    tablet
        .wait_until("the lobby unmuted on the tablet", |c| {
            c.is_muted(ChatId::MAIN).map(|m| !m).unwrap_or(false)
        })
        .await;
}
