//! client-runtime: Unsent messages query. The app asks it when it goes to the
//! background, to finish sending first (change `finish-sends-in-background`).

mod support;

use client::{ConnectionState, Event, SendStatus};
use support::Check;
use support::device;
use support::server::TestServer;

// spec: client-runtime/unsent-messages-query
#[tokio::test(flavor = "multi_thread")]
async fn a_message_waiting_to_send_is_unsent_until_the_server_accepts_it() {
    let server = TestServer::start().await;
    let mut admin = device::admin_with_group(&server).await;
    assert!(!admin.client.has_unsent().check("nothing sent yet"));

    // No connection: the message is stored and waits.
    server.relay.cut();
    admin
        .wait_event("the drop", |e| {
            matches!(e, Event::Connection(ConnectionState::Connecting))
        })
        .await;
    let id = admin.client.send_text("hi").check("send");
    assert!(admin.client.has_unsent().check("query"), "waiting to send");

    // Back online: accepted, and nothing is unsent any more.
    server.relay.restore();
    admin
        .wait_event(
            "accepted",
            |e| matches!(e, Event::SendStatus { id: i, status: SendStatus::Accepted } if *i == id),
        )
        .await;
    assert!(!admin.client.has_unsent().check("query"), "all accepted");
}
