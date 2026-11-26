//! moderation end to end against the real server (change `report-and-block`):
//! blocking a user hides their messages and DM on the blocker's device only and
//! writes no RPC; reporting a message or a user reaches the admin over a DM as a
//! report `Content.Text`, referencing group content and quoting DM content; and
//! the admin acts on a report with the existing removal.
//!
//! The in-app presentation (app-ui: Block and unblock a user; Report a message
//! or a user; The admin sees reports; Safety and moderation entry) is the app's
//! job (widget tests). The core keeps the block list, filters blocked senders,
//! composes and sends the report, and parses it, which these tests cover.

mod support;

use client::{
    ChatKind, Event, Message, MessageBody, MessageId, Report, ReportKind, ReportTarget, SendStatus,
};
use support::Check;
use support::device::{self, Device, member, user};
use support::server::TestServer;

/// The admin, Alice and Mallory, all in the group and seeing each other.
async fn group(server: &TestServer) -> (Device, Device, Device) {
    let admin = device::admin_with_group(server).await;
    let alice = device::member_of_group(&admin, "alice").await;
    let mallory = device::member_of_group(&admin, "mallory").await;
    let everyone = [member(&admin), member(&alice), member(&mallory)];
    for d in [&admin, &alice, &mallory] {
        d.wait_members(&everyone).await;
    }
    (admin, alice, mallory)
}

async fn wait_text(device: &mut Device, text: &str) -> Message {
    let wanted = MessageBody::Text(text.to_owned());
    match device
        .wait_event(
            &format!("the message {text:?}"),
            |e| matches!(e, Event::Message(m) if m.body == wanted),
        )
        .await
    {
        Event::Message(message) => message,
        _ => unreachable!(),
    }
}

async fn wait_accepted(device: &mut Device, id: MessageId) {
    device
        .wait_event(
            "the message accepted",
            |e| matches!(e, Event::SendStatus { id: i, status: SendStatus::Accepted } if *i == id),
        )
        .await;
}

/// Alice's view of the lobby history, as plain text bodies.
fn lobby_texts(device: &Device) -> Vec<String> {
    device
        .client
        .history(50)
        .check("history")
        .into_iter()
        .filter_map(|m| match m.body {
            MessageBody::Text(t) => Some(t),
            _ => None,
        })
        .collect()
}

// spec: moderation/block-a-user
#[tokio::test(flavor = "multi_thread")]
async fn block_and_unblock_are_local_and_keep_membership() {
    let server = TestServer::start().await;
    let (_admin, alice, mallory) = group(&server).await;

    assert!(alice.client.blocked_users().check("blocked").is_empty());

    alice.client.block_user(&user("mallory")).check("block");
    assert!(alice.client.is_blocked(&user("mallory")).check("is?"));
    assert_eq!(
        alice.client.blocked_users().check("blocked"),
        vec![user("mallory")]
    );
    // Blocking never touches membership: Mallory stays in the group for both.
    let everyone = [member(&alice), member(&mallory)];
    for user in &everyone {
        assert!(alice.client.members().check("members").contains(user));
        assert!(mallory.client.members().check("members").contains(user));
    }

    alice.client.unblock_user(&user("mallory")).check("unblock");
    assert!(!alice.client.is_blocked(&user("mallory")).check("is?"));
    assert!(alice.client.blocked_users().check("blocked").is_empty());
}

// spec: moderation/block-a-user
#[tokio::test(flavor = "multi_thread")]
async fn blocking_does_not_reach_a_second_device() {
    let server = TestServer::start().await;
    let (_admin, alice, _mallory) = group(&server).await;

    alice.client.block_user(&user("mallory")).check("block");

    // A linked tablet joins; the block never synced, so its list is empty.
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
    assert!(tablet.client.blocked_users().check("blocked").is_empty());
    assert!(!tablet.client.is_blocked(&user("mallory")).check("is?"));
}

// spec: moderation/a-blocked-user-is-hidden
#[tokio::test(flavor = "multi_thread")]
async fn a_blocked_senders_group_message_is_hidden_and_reappears() {
    let server = TestServer::start().await;
    let (_admin, mut alice, mut mallory) = group(&server).await;

    alice.client.block_user(&user("mallory")).check("block");

    let id = mallory.client.send_text("hello all").check("send");
    wait_accepted(&mut mallory, id).await;

    // Alice gets no message event and nothing in her history.
    assert!(
        !alice
            .sees_within_quiet(|e| matches!(e, Event::Message(m)
                if m.body == MessageBody::Text("hello all".into())))
            .await
    );
    assert!(!lobby_texts(&alice).contains(&"hello all".to_string()));

    // Unblocking reveals the stored message again.
    alice.client.unblock_user(&user("mallory")).check("unblock");
    assert!(lobby_texts(&alice).contains(&"hello all".to_string()));
}

// spec: moderation/a-blocked-user-is-hidden
#[tokio::test(flavor = "multi_thread")]
async fn a_blocked_dm_peer_disappears_from_the_chat_list() {
    let server = TestServer::start().await;
    let (_admin, mut alice, mut mallory) = group(&server).await;

    let chat = alice
        .client
        .open_dm(&user("mallory"))
        .await
        .check("open DM");
    let id = alice
        .client
        .send_message_in(chat, "hi", &[], None)
        .check("send");
    wait_accepted(&mut alice, id).await;
    wait_text(&mut mallory, "hi").await;

    let has_mallory_dm = |d: &Device| {
        d.client
            .chats()
            .check("chats")
            .into_iter()
            .any(|c| matches!(&c.kind, ChatKind::Dm { peer, .. } if *peer == user("mallory")))
    };
    assert!(has_mallory_dm(&alice));

    alice.client.block_user(&user("mallory")).check("block");
    assert!(!has_mallory_dm(&alice));

    alice.client.unblock_user(&user("mallory")).check("unblock");
    assert!(has_mallory_dm(&alice));
}

/// Waits for a report DM to reach `admin` and returns the parsed [`Report`].
async fn wait_report(admin: &mut Device) -> Report {
    let message = admin
        .wait_event("a report DM", |e| {
            matches!(e, Event::Message(m)
                if matches!(&m.body, MessageBody::Text(t)
                    if t.starts_with("[vyajena-report v1]")))
        })
        .await;
    let Event::Message(m) = message else {
        unreachable!()
    };
    let MessageBody::Text(text) = m.body else {
        unreachable!()
    };
    admin.client.parse_report(&text).check("parsed report")
}

// spec: moderation/report-a-message
#[tokio::test(flavor = "multi_thread")]
async fn a_group_message_report_references_it_to_the_admin() {
    let server = TestServer::start().await;
    let (mut admin, mut alice, mut mallory) = group(&server).await;

    let id = mallory.client.send_text("rude thing").check("send");
    wait_accepted(&mut mallory, id).await;
    let seen = wait_text(&mut alice, "rude thing").await;

    alice
        .client
        .report(ReportTarget::Message(seen.id), "harassment")
        .await
        .check("report");

    let report = wait_report(&mut admin).await;
    assert_eq!(report.reported, user("mallory"));
    assert_eq!(report.reason, "harassment");
    match report.kind {
        ReportKind::Group { group_id, msg_id } => {
            assert!(!group_id.is_empty());
            assert!(!msg_id.is_empty());
        }
        other => panic!("expected a group reference, got {other:?}"),
    }
}

// spec: moderation/report-a-dm-message
#[tokio::test(flavor = "multi_thread")]
async fn a_dm_message_report_quotes_it_to_the_admin() {
    let server = TestServer::start().await;
    let (mut admin, mut alice, mut mallory) = group(&server).await;

    // Mallory DMs Alice something abusive.
    let chat = mallory
        .client
        .open_dm(&user("alice"))
        .await
        .check("open DM");
    let id = mallory
        .client
        .send_message_in(chat, "you are awful", &[], None)
        .check("send DM");
    wait_accepted(&mut mallory, id).await;
    let seen = wait_text(&mut alice, "you are awful").await;

    alice
        .client
        .report(ReportTarget::Message(seen.id), "abuse")
        .await
        .check("report");

    let report = wait_report(&mut admin).await;
    assert_eq!(report.reported, user("mallory"));
    match report.kind {
        ReportKind::Dm { quote } => assert_eq!(quote, "you are awful"),
        other => panic!("expected a DM quote, got {other:?}"),
    }
}

// spec: moderation/report-a-dm-message
#[tokio::test(flavor = "multi_thread")]
async fn a_long_dm_report_quote_is_cut_to_the_limit() {
    let server = TestServer::start().await;
    let (mut admin, mut alice, mut mallory) = group(&server).await;

    let long = "x".repeat(protocol::message_content::MAX_REPLY_QUOTE_CHARS + 50);
    let chat = mallory
        .client
        .open_dm(&user("alice"))
        .await
        .check("open DM");
    let id = mallory
        .client
        .send_message_in(chat, &long, &[], None)
        .check("send DM");
    wait_accepted(&mut mallory, id).await;
    let seen = wait_text(&mut alice, &long).await;

    alice
        .client
        .report(ReportTarget::Message(seen.id), "spam")
        .await
        .check("report");

    let report = wait_report(&mut admin).await;
    match report.kind {
        ReportKind::Dm { quote } => {
            assert_eq!(
                quote.chars().count(),
                protocol::message_content::MAX_REPLY_QUOTE_CHARS
            );
            assert!(quote.ends_with('…'));
        }
        other => panic!("expected a DM quote, got {other:?}"),
    }
}

// spec: moderation/report-a-user
#[tokio::test(flavor = "multi_thread")]
async fn a_user_report_names_only_the_user_and_reason() {
    let server = TestServer::start().await;
    let (mut admin, alice, _mallory) = group(&server).await;

    alice
        .client
        .report(ReportTarget::User(user("mallory")), "repeated abuse")
        .await
        .check("report");

    let report = wait_report(&mut admin).await;
    assert_eq!(report.reported, user("mallory"));
    assert_eq!(report.reason, "repeated abuse");
    assert_eq!(report.kind, ReportKind::User);
}

// spec: moderation/a-report-reaches-the-admin
#[tokio::test(flavor = "multi_thread")]
async fn a_report_waits_for_an_offline_admin() {
    let server = TestServer::start().await;
    let (mut admin, alice, _mallory) = group(&server).await;

    // The admin goes offline, then Alice reports.
    admin.client.stop();
    alice
        .client
        .report(ReportTarget::User(user("mallory")), "offline case")
        .await
        .check("report");

    // On the admin's next connect the report is delivered, shown as a report.
    admin.client.start().await.check("restart admin");
    let report = wait_report(&mut admin).await;
    assert_eq!(report.reported, user("mallory"));
    assert_eq!(report.reason, "offline case");
}

// spec: moderation/the-admin-acts-on-a-report-with-the-existing-removal
#[tokio::test(flavor = "multi_thread")]
async fn the_admin_removes_a_reported_user_with_the_existing_flow() {
    let server = TestServer::start().await;
    let (mut admin, alice, _mallory) = group(&server).await;

    alice
        .client
        .report(ReportTarget::User(user("mallory")), "ban please")
        .await
        .check("report");
    let report = wait_report(&mut admin).await;
    assert_eq!(report.reported, user("mallory"));

    // Acting on it is the existing removal; no new moderation RPC.
    admin
        .client
        .remove_user(&report.reported)
        .await
        .check("remove the reported user");
    admin
        .wait_until("mallory removed", |c| {
            c.members()
                .map(|members| members.iter().all(|m| m.user != user("mallory")))
                .unwrap_or(false)
        })
        .await;
}
