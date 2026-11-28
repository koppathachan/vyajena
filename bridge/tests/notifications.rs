//! local-notifications through the bridge (change `smarter-notifications`): the
//! summary Dart reads after the core catches up. Metadata only: the bridge types
//! have no sender or text field.

mod support;

use bridge::api::types::{CoreEvent, NotificationConversationKind};
use support::Check;
use support::device::{self, Device};
use support::server::TestServer;

// spec: local-notifications/the-device-builds-the-notification-locally
#[tokio::test(flavor = "multi_thread")]
async fn the_summary_reaches_dart_as_a_type_and_a_count_only() {
    let server = TestServer::start().await;
    let mut admin = device::admin_with_group(&server).await;
    let mut alice: Device = device::member_of_group(&admin, "alice").await;
    admin.core.send_text("a secret agenda".into()).check("send");
    alice
        .wait_event("the message", |e| matches!(e, CoreEvent::Message { .. }))
        .await;
    let _ = &mut admin;

    let summary = alice.core.notification_summary().check("summary");
    assert_eq!(summary.conversations.len(), 1);
    let lobby = &summary.conversations[0];
    assert_eq!(lobby.kind, NotificationConversationKind::Lobby);
    assert_eq!((lobby.chat_id, lobby.unread), (0, 1));
    assert_eq!(summary.total_unread, 1);
    assert!(summary.admin_events.is_empty());
    let shown = format!("{summary:?}").to_lowercase();
    assert!(
        !shown.contains("secret") && !shown.contains("agenda"),
        "{shown}"
    );
}
