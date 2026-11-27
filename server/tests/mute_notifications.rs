//! notification-prefs on the server (change `mute-notifications`): the private
//! per-user mute set, its sync on connect, and the wake push a muted
//! conversation does not get.
//!
//! Cast (support::messaging::World): the admin (`akhil`), Alice's laptop and
//! Bob's phone, all in the main group (`GROUP_ID`).

mod support;

use std::time::Duration;

use support::Check;
use support::clients::TestDevice;
use support::harness::{Conn, TestServer, authed};
use support::messaging::{GROUP_ID, World, offline, receive, reconnect, say, wait_offline, world};
use support::pb;
use tonic::Code;

/// Sets `device`'s FCM push token.
async fn set_token(conn: &Conn, device: &TestDevice, token: &str) {
    conn.devices()
        .update_push_token(authed(
            device.token(),
            pb::UpdatePushTokenRequest {
                push_token: Some(pb::PushToken {
                    platform: pb::PushPlatform::Fcm as i32,
                    token: token.to_owned(),
                }),
            },
        ))
        .await
        .check("UpdatePushToken");
}

/// Calls `Prefs.SetMutePref` as `caller`.
async fn set_mute(
    w: &World,
    caller: &TestDevice,
    scope: pb::MuteScope,
    group_id: &[u8],
    muted: bool,
) -> Result<(), tonic::Status> {
    w.conn
        .prefs()
        .set_mute_pref(authed(
            caller.token(),
            pb::SetMutePrefRequest {
                scope: scope as i32,
                group_id: group_id.to_vec(),
                muted,
            },
        ))
        .await
        .map(|_| ())
}

/// Waits until `device` has at least `expected` pings.
async fn wait_pings(server: &TestServer, device: &TestDevice, expected: usize) {
    let deadline = tokio::time::Instant::now() + support::messaging::WAIT;
    while server.pushes.count(&device.device_id) < expected {
        assert!(
            tokio::time::Instant::now() < deadline,
            "{} of {expected} pings",
            server.pushes.count(&device.device_id)
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

// spec: notification-prefs/mute-preference-sync
#[tokio::test]
async fn the_mute_set_is_sent_as_one_snapshot_on_every_connect() {
    let mut w = world().await;
    set_mute(&w, &w.laptop, pb::MuteScope::Conversation, GROUP_ID, true)
        .await
        .check("SetMutePref");

    // Every connect names it again: a snapshot, not a queue (no ack).
    for _ in 0..2 {
        w.laptop_stream = reconnect(&w.conn, &mut w.laptop).await;
        let prefs = w.laptop_stream.mute_prefs().await;
        assert_eq!(prefs.muted_group_ids, vec![GROUP_ID.to_vec()]);
        assert!(!prefs.muted_all);
    }
}

// spec: notification-prefs/mute-preference-sync
#[tokio::test]
async fn whole_app_mute_rides_the_same_snapshot() {
    let mut w = world().await;
    set_mute(&w, &w.laptop, pb::MuteScope::Global, &[], true)
        .await
        .check("SetMutePref");

    w.laptop_stream = reconnect(&w.conn, &mut w.laptop).await;
    let prefs = w.laptop_stream.mute_prefs().await;
    assert!(prefs.muted_group_ids.is_empty());
    assert!(prefs.muted_all);

    // Clearing it is reflected on the next connect.
    set_mute(&w, &w.laptop, pb::MuteScope::Global, &[], false)
        .await
        .check("SetMutePref");
    w.laptop_stream = reconnect(&w.conn, &mut w.laptop).await;
    assert!(!w.laptop_stream.mute_prefs().await.muted_all);
}

// spec: notification-prefs/mute-preference-sync
#[tokio::test]
async fn only_the_users_own_set_is_sent_and_it_holds_ids_and_a_flag_only() {
    let mut w = world().await;
    set_mute(&w, &w.laptop, pb::MuteScope::Conversation, GROUP_ID, true)
        .await
        .check("SetMutePref");

    // Bob connects: his own snapshot is empty; he never learns Alice's set.
    w.phone_stream = reconnect(&w.conn, &mut w.phone).await;
    let bob = w.phone_stream.mute_prefs().await;
    assert!(bob.muted_group_ids.is_empty());
    assert!(!bob.muted_all);

    // The server holds only the ids and the one flag.
    let conversations: Vec<String> = w
        .server
        .query(|db| {
            let mut stmt =
                db.prepare("SELECT name FROM pragma_table_info('muted_conversations')")?;
            stmt.query_map([], |r| r.get(0))?.collect()
        })
        .await;
    assert_eq!(conversations, ["username", "group_id"]);
    let flag: Vec<String> = w
        .server
        .query(|db| {
            let mut stmt = db.prepare("SELECT name FROM pragma_table_info('muted_all')")?;
            stmt.query_map([], |r| r.get(0))?.collect()
        })
        .await;
    assert_eq!(flag, ["username"]);
}

// spec: notification-prefs/mute-preference-sync
#[tokio::test]
async fn muting_twice_is_idempotent() {
    let w = world().await;
    for _ in 0..2 {
        set_mute(&w, &w.laptop, pb::MuteScope::Conversation, GROUP_ID, true)
            .await
            .check("SetMutePref");
    }
    let rows: i64 = w
        .server
        .query(|db| db.query_row("SELECT count(*) FROM muted_conversations", [], |r| r.get(0)))
        .await;
    assert_eq!(rows, 1);
}

// spec: notification-prefs/mute-a-conversation
#[tokio::test]
async fn only_a_conversation_you_are_in_can_be_muted() {
    let w = world().await;

    // An unknown id, and so a conversation the caller is not a member of, is
    // refused: ids of others' conversations are not probed.
    let unknown = set_mute(
        &w,
        &w.laptop,
        pb::MuteScope::Conversation,
        b"no-such-conversation",
        true,
    )
    .await
    .unwrap_err();
    assert_eq!(unknown.code(), Code::PermissionDenied);

    // Nothing was recorded.
    let rows: i64 = w
        .server
        .query(|db| db.query_row("SELECT count(*) FROM muted_conversations", [], |r| r.get(0)))
        .await;
    assert_eq!(rows, 0);

    // The lobby, which Bob is in, is accepted.
    set_mute(&w, &w.phone, pb::MuteScope::Conversation, GROUP_ID, true)
        .await
        .check("SetMutePref");
}

// spec: notification-prefs/a-muted-conversation-gets-no-wake-push
#[tokio::test]
async fn a_muted_group_gets_no_wake_push_but_the_message_still_arrives() {
    let mut w = world().await;
    set_token(&w.conn, &w.phone, "phone-fcm-token").await;
    set_token(&w.conn, &w.laptop, "laptop-fcm-token").await;

    // Bob mutes the lobby; Alice does not.
    set_mute(&w, &w.phone, pb::MuteScope::Conversation, GROUP_ID, true)
        .await
        .check("SetMutePref");

    // Both go offline, then the admin sends to the lobby.
    w.phone_stream = offline(&w.conn, &w.phone).await;
    w.laptop_stream = offline(&w.conn, &w.laptop).await;
    wait_offline(&w.server, &w.phone).await;
    wait_offline(&w.server, &w.laptop).await;
    say(&mut w.admin, &mut w.admin_stream, b"hello lobby").await;

    // Alice (unmuted) is pinged; both devices were decided in the same pass, so
    // once Alice's ping lands, Bob's (muted) skip has already happened.
    wait_pings(&w.server, &w.laptop, 1).await;
    assert_eq!(w.server.pushes.count(&w.phone.device_id), 0);

    // The message was only unpinged, not dropped: Bob gets it on reconnect.
    w.phone_stream = reconnect(&w.conn, &mut w.phone).await;
    let envelope = receive(&mut w.phone_stream, &mut w.phone).await;
    assert_eq!(envelope.group_id, GROUP_ID);
}

// spec: notification-prefs/a-muted-conversation-gets-no-wake-push
#[tokio::test]
async fn mute_all_suppresses_every_push() {
    let mut w = world().await;
    set_token(&w.conn, &w.phone, "phone-fcm-token").await;
    set_token(&w.conn, &w.laptop, "laptop-fcm-token").await;

    // Bob mutes everything; Alice does not.
    set_mute(&w, &w.phone, pb::MuteScope::Global, &[], true)
        .await
        .check("SetMutePref");

    w.phone_stream = offline(&w.conn, &w.phone).await;
    w.laptop_stream = offline(&w.conn, &w.laptop).await;
    wait_offline(&w.server, &w.phone).await;
    wait_offline(&w.server, &w.laptop).await;
    say(&mut w.admin, &mut w.admin_stream, b"anything").await;

    wait_pings(&w.server, &w.laptop, 1).await;
    assert_eq!(w.server.pushes.count(&w.phone.device_id), 0);
}

// spec: notification-prefs/a-muted-conversation-gets-no-wake-push
#[tokio::test]
async fn an_unmuted_conversation_still_pings() {
    let mut w = world().await;
    set_token(&w.conn, &w.phone, "phone-fcm-token").await;
    w.phone_stream = offline(&w.conn, &w.phone).await;
    wait_offline(&w.server, &w.phone).await;

    say(&mut w.admin, &mut w.admin_stream, b"wake up").await;
    wait_pings(&w.server, &w.phone, 1).await;
    assert_eq!(w.server.pushes.count(&w.phone.device_id), 1);
}

// spec: notification-prefs/a-muted-conversation-gets-no-wake-push
#[tokio::test]
async fn a_backlog_only_in_a_muted_conversation_is_never_swept() {
    // A muted conversation's queued message must not be turned into a wake push
    // later by the catch-up sweep: the device has nothing it may show.
    let mut w = world().await;
    set_token(&w.conn, &w.phone, "phone-fcm-token").await;
    set_mute(&w, &w.phone, pb::MuteScope::Conversation, GROUP_ID, true)
        .await
        .check("SetMutePref");
    w.phone_stream = offline(&w.conn, &w.phone).await;
    wait_offline(&w.server, &w.phone).await;

    say(&mut w.admin, &mut w.admin_stream, b"muted news").await;
    for _ in 0..3 {
        w.server.clock.advance(Duration::from_secs(61));
        w.server.sweep().await;
    }
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(w.server.pushes.count(&w.phone.device_id), 0);

    // The message was only unpinged, not dropped.
    w.phone_stream = reconnect(&w.conn, &mut w.phone).await;
    let envelope = receive(&mut w.phone_stream, &mut w.phone).await;
    assert_eq!(envelope.group_id, GROUP_ID);
}

// spec: notification-prefs/a-muted-conversation-gets-no-wake-push
#[tokio::test]
async fn a_muted_backlog_gets_no_catch_up_ping_at_stream_loss() {
    // The stream is lost with only a muted conversation's entry undelivered: the
    // re-ping on stream loss must see that there is nothing to show.
    let mut w = world().await;
    set_token(&w.conn, &w.phone, "phone-fcm-token").await;
    set_mute(&w, &w.phone, pb::MuteScope::Conversation, GROUP_ID, true)
        .await
        .check("SetMutePref");
    // Queued while the phone's stream is open and never acked.
    say(&mut w.admin, &mut w.admin_stream, b"muted, while open").await;
    w.phone_stream = offline(&w.conn, &w.phone).await;
    wait_offline(&w.server, &w.phone).await;
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(w.server.pushes.count(&w.phone.device_id), 0);
}
