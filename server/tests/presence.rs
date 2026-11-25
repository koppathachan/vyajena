//! manual-presence on the server (change `manual-presence`): the durable manual
//! flag, its broadcast to co-members, the on-connect snapshot, the version-4
//! gate and durability across disconnect.
//!
//! Cast (support::messaging::World): the admin (`akhil`), Alice's laptop and
//! Bob's phone, all in the main group (`GROUP_ID`).

mod support;

use support::messaging::{Stream, World, offline, reconnect, world};
use support::pb;

/// The wire value of `PRESENCE_STATUS_ONLINE`.
const ONLINE: i32 = pb::PresenceStatus::Online as i32;
/// The wire value of `PRESENCE_STATUS_OFFLINE`.
const OFFLINE: i32 = pb::PresenceStatus::Offline as i32;

/// Alice's stored flag in the server database.
async fn stored(w: &World, username: &str) -> i32 {
    w.server
        .query({
            let username = username.to_owned();
            move |db| {
                db.query_row(
                    "SELECT manual_presence FROM users WHERE username = ?1",
                    [username],
                    |r| r.get::<_, i64>(0),
                )
            }
        })
        .await as i32
}

// spec: presence/setting-your-status
// spec: presence/broadcasting-a-status-change
#[tokio::test]
async fn set_presence_persists_and_a_co_member_is_told() {
    let mut w = world().await;

    // Alice sets Online on her laptop stream.
    w.laptop_stream.send_presence(ONLINE).await;

    // Bob, who shares the lobby with Alice, is told.
    let update = w.phone_stream.presence_update().await;
    assert_eq!(update.user.unwrap().username, "alice");
    assert_eq!(update.status, ONLINE);
    assert!(update.updated_at.is_some());

    // The admin, also a co-member, is told too.
    let to_admin = w.admin_stream.presence_update().await;
    assert_eq!(to_admin.user.unwrap().username, "alice");
    assert_eq!(to_admin.status, ONLINE);

    // The flag is stored durably as Online.
    assert_eq!(stored(&w, "alice").await, ONLINE);

    // Alice is not told about her own change (others only).
    w.laptop_stream.assert_no_presence_update().await;
}

// spec: presence/broadcasting-a-status-change
#[tokio::test]
async fn a_non_sharing_user_is_not_told() {
    let mut w = world().await;

    // Carol registers but is never added to the lobby, so she shares no
    // conversation with Alice. She connects on a version-4 stream.
    let (_carol, carol_device) = support::invite_user(&w.conn, &w.admin, "carol").await;
    let mut carol_stream = Stream::open(&w.conn, &carol_device).await;
    carol_stream.sync().await;

    w.laptop_stream.send_presence(ONLINE).await;

    // Bob (shares the lobby) is told; Carol (shares nothing) is not.
    let update = w.phone_stream.presence_update().await;
    assert_eq!(update.user.unwrap().username, "alice");
    carol_stream.assert_no_presence_update().await;
}

// spec: presence/status-snapshot-on-connect
#[tokio::test]
async fn a_reconnect_gets_the_snapshot() {
    let mut w = world().await;
    w.laptop_stream.send_presence(ONLINE).await;
    // Let the write land before Bob reconnects.
    let _ = w.phone_stream.presence_update().await;

    // Bob reconnects: before any live traffic his snapshot reports every user he
    // may see, Alice at Online.
    w.phone_stream = reconnect(&w.conn, &mut w.phone).await;
    let snapshot = w.phone_stream.drain_presence_updates();
    let alice = snapshot
        .iter()
        .find(|u| u.user.as_ref().unwrap().username == "alice")
        .expect("snapshot names Alice");
    assert_eq!(alice.status, ONLINE);
    // The snapshot is about others; it does not include Bob himself.
    assert!(
        snapshot
            .iter()
            .all(|u| u.user.as_ref().unwrap().username != "bob")
    );
}

// spec: presence/default-status-before-first-set
#[tokio::test]
async fn a_user_who_never_set_one_is_reported_at_the_default_offline() {
    let mut w = world().await;

    // Nobody has set a flag: Bob's connect snapshot reports his co-members
    // (Alice, the admin) at the owner-chosen default, Offline.
    w.phone_stream = reconnect(&w.conn, &mut w.phone).await;
    let snapshot = w.phone_stream.drain_presence_updates();
    assert!(!snapshot.is_empty(), "a snapshot is sent");
    assert!(snapshot.iter().all(|u| u.status == OFFLINE));
    assert_eq!(stored(&w, "alice").await, OFFLINE);
}

// spec: presence/broadcasting-a-status-change
#[tokio::test]
async fn an_old_version_3_client_gets_no_presence_update() {
    let mut w = world().await;

    // Bob reconnects on a version-3 stream (an older client).
    w.phone.authenticate(&w.conn).await;
    let mut old = Stream::open_version(&w.conn, &w.phone, 3).await;
    old.sync().await;
    // A v3 connect sends no snapshot.
    assert!(old.drain_presence_updates().is_empty());

    // Alice sets Online: the v3 stream is sent no PresenceUpdate.
    w.laptop_stream.send_presence(ONLINE).await;
    old.assert_no_presence_update().await;
    // The flag is still stored (the broadcast is simply gated, not dropped).
    assert_eq!(stored(&w, "alice").await, ONLINE);
}

// spec: presence/durable-per-user-status-storage
// spec: presence/manual-status-never-inferred
#[tokio::test]
async fn the_flag_survives_the_user_going_offline() {
    let mut w = world().await;
    w.laptop_stream.send_presence(ONLINE).await;
    let _ = w.phone_stream.presence_update().await;

    // Alice's device disconnects: an offline stream (no Hello) replaces and so
    // closes her real one.
    let _laptop_offline = offline(&w.conn, &w.laptop).await;

    // The stored flag is unchanged, and Bob's next snapshot still reports Online.
    assert_eq!(stored(&w, "alice").await, ONLINE);
    w.phone_stream = reconnect(&w.conn, &mut w.phone).await;
    let snapshot = w.phone_stream.drain_presence_updates();
    let alice = snapshot
        .iter()
        .find(|u| u.user.as_ref().unwrap().username == "alice")
        .expect("snapshot names Alice");
    assert_eq!(alice.status, ONLINE);
    drop(_laptop_offline);
}

// spec: presence/setting-your-status
#[tokio::test]
async fn an_unspecified_status_is_ignored() {
    let mut w = world().await;
    // An UNSPECIFIED SetPresence sets nothing and tells no one.
    w.laptop_stream
        .send_presence(pb::PresenceStatus::Unspecified as i32)
        .await;
    w.phone_stream.assert_no_presence_update().await;
    // The stored value stays the default.
    assert_eq!(stored(&w, "alice").await, OFFLINE);
}
