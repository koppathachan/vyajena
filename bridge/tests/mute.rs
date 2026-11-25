//! Muting through the bridge (notification-prefs, change `mute-notifications`):
//! muting and unmuting a conversation, the muted-row marker, and whole-app mute.

mod support;

use bridge::api::types::CoreEvent;
use support::Check;
use support::device::{self, Device};
use support::server::TestServer;

/// Whether the lobby row (chat id 0) is marked muted.
fn lobby_muted(device: &Device) -> bool {
    device
        .core
        .chats()
        .check("chats")
        .into_iter()
        .find(|c| c.chat_id == 0)
        .is_some_and(|c| c.muted)
}

// spec: notification-prefs/mute-a-conversation
// spec: notification-prefs/mute-the-whole-app
#[tokio::test(flavor = "multi_thread")]
async fn mute_a_conversation_and_the_whole_app_through_the_bridge() {
    let server = TestServer::start().await;
    let mut alice = device::admin_with_group(&server).await;

    assert!(!alice.core.app_muted().check("app muted?"));
    assert!(!lobby_muted(&alice));

    // Mute the lobby.
    alice
        .core
        .set_conversation_muted(0, true)
        .await
        .check("mute lobby");
    alice
        .wait_event("the mute set changed", |e| {
            matches!(e, CoreEvent::MutePrefsChanged)
        })
        .await;
    assert!(lobby_muted(&alice));
    assert!(!alice.core.app_muted().check("app muted?"));

    // Unmute it.
    alice
        .core
        .set_conversation_muted(0, false)
        .await
        .check("unmute lobby");
    assert!(!lobby_muted(&alice));

    // Whole-app mute.
    alice.core.set_app_muted(true).await.check("mute all");
    assert!(alice.core.app_muted().check("app muted?"));
    alice
        .core
        .set_app_muted(false)
        .await
        .check("clear mute all");
    assert!(!alice.core.app_muted().check("app muted?"));
}
