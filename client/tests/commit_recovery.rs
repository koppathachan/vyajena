//! client-runtime: Resume a staged own commit across a restart. End to end
//! against the real server: a membership commit this device staged but had not
//! yet merged is recovered after a crash as exactly the job that staged it —
//! the same group and the same chained follow-up — and an own commit whose
//! stored record no longer parses is cleared, never reinterpreted.
//!
//! The plain in-flight add-and-resend case is covered in `group_membership.rs`
//! (`a_commit_in_flight_survives_a_crash_and_is_resent_identically`), which now
//! also exercises the typed codec's round-trip for `AddUser`.

mod support;

use client::{Chat, ChatId, ChatKind};
use support::Check;
use support::device::{self, Device, member, user};
use support::server::TestServer;

/// The stored record of the own commit in flight (the `outgoing_commit.purpose`
/// column), or `None` if there is no commit in flight. The column is plain text
/// (only MLS values are sealed), so a test can read it directly.
fn in_flight_purpose(device: &Device) -> Option<String> {
    let path = device.data_dir().join(client::DATABASE_FILE);
    let conn =
        rusqlite::Connection::open_with_flags(&path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
            .ok()?;
    conn.query_row("SELECT purpose FROM outgoing_commit", [], |row| {
        row.get::<_, String>(0)
    })
    .ok()
}

fn commit_in_flight(device: &Device) -> bool {
    in_flight_purpose(device).is_some()
}

fn group_named(device: &Device, wanted: &str) -> Option<Chat> {
    device
        .client
        .chats()
        .check("chats")
        .into_iter()
        .find(|c| matches!(&c.kind, ChatKind::Group { name: Some(n), .. } if n == wanted))
}

async fn wait_group(device: &Device, name: &str) -> ChatId {
    device
        .wait_until("the named group", |c| {
            c.chats().is_ok_and(|chats| {
                chats.iter().any(
                    |chat| matches!(&chat.kind, ChatKind::Group { name: Some(n), .. } if n == name),
                )
            })
        })
        .await;
    group_named(device, name).check("the group").id
}

async fn wait_members_of(admin: &Device, chat: ChatId, who: &[&Device]) {
    let mut wanted: Vec<_> = who.iter().map(|d| member(d)).collect();
    wanted.push(member(admin));
    wanted.sort();
    admin
        .wait_until("the group's members", |c| {
            c.group_members(chat).is_ok_and(|m| m == wanted)
        })
        .await;
}

async fn device_revoked(server: &TestServer, device: [u8; 16]) -> bool {
    server
        .query(move |conn| {
            conn.query_row(
                "SELECT revoked_at IS NOT NULL FROM devices WHERE device_id = ?1",
                [device.as_slice()],
                |row| row.get(0),
            )
        })
        .await
}

/// A user removal that is in flight on the *named-group* commit carries the
/// follow-up `Admin.RemoveUser` in the job's `after`. A crash there must, on
/// restart, finish the named-group removal **and** run the chained
/// `Admin.RemoveUser`, so the user ends up fully removed — not left removed from
/// the group only. The old string codec dropped `after` and lost the follow-up.
// spec: client-runtime/resume-a-staged-own-commit-across-a-restart
#[tokio::test(flavor = "multi_thread")]
async fn a_group_removal_keeps_its_follow_up_across_a_crash() {
    let server = TestServer::start().await;
    let mut admin = device::admin_with_group(&server).await;
    let alice = device::member_of_group(&admin, "alice").await;
    let bob = device::member_of_group(&admin, "bob").await;
    let everyone = [member(&admin), member(&alice), member(&bob)];
    admin.wait_members(&everyone).await;
    let bob_device = bob.device_id();

    let chat = admin
        .client
        .create_named_group(Some("Weekend"), &[user("alice"), user("bob")])
        .await
        .check("create the group");
    wait_group(&bob, "Weekend").await;
    wait_members_of(&admin, chat, &[&alice, &bob]).await;

    // Remove Bob. The lobby (main-group) removal commits first; its merge then
    // queues the named-group removal with `Admin.RemoveUser` chained after it.
    // Crash the admin while that named-group commit is in flight.
    let removing = {
        let user = user("bob");
        let client = &admin.client;
        async move { client.remove_user(&user).await }
    };
    let watch = async {
        let deadline = tokio::time::Instant::now() + support::device::WAIT;
        loop {
            // The named-group removal is the commit that carries the follow-up.
            if in_flight_purpose(&admin).is_some_and(|p| p.contains("RemoveFromGroup")) {
                break;
            }
            assert!(
                tokio::time::Instant::now() < deadline,
                "the named-group removal never reached the commit-in-flight state"
            );
            tokio::time::sleep(std::time::Duration::from_millis(1)).await;
        }
        server.relay.cut();
    };
    let (result, ()) = tokio::join!(
        tokio::time::timeout(std::time::Duration::from_secs(5), removing),
        watch
    );
    drop(result);
    admin.restart().await;
    server.relay.restore();

    // After the crash the admin finishes the named-group removal and then runs
    // the chained Admin.RemoveUser: Bob leaves the group, leaves the lobby, and
    // his device is revoked on the server (the effect of Admin.RemoveUser).
    wait_members_of(&admin, chat, &[&alice]).await;
    admin
        .wait_until("the lobby without Bob", |c| {
            c.members()
                .is_ok_and(|m| m.iter().all(|m| m.user != user("bob")))
        })
        .await;
    // Admin.RemoveUser revokes Bob's device on the server. Without the `after`
    // round-trip it never runs, so the device stays unrevoked.
    let deadline = tokio::time::Instant::now() + support::device::WAIT;
    while !device_revoked(&server, bob_device).await {
        assert!(
            tokio::time::Instant::now() < deadline,
            "Admin.RemoveUser did not run after the crash: the follow-up was lost"
        );
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    }
    assert!(!commit_in_flight(&admin));
}

/// An own commit whose stored record cannot be parsed into a known job must be
/// cleared on restart — the staged commit and the row dropped — never resumed as
/// a different commit. The old codec resumed any unrecognised record as a
/// self-rotation; here the staged removal is discarded instead, so the target
/// user stays a member and no rotation is sent.
// spec: client-runtime/resume-a-staged-own-commit-across-a-restart
#[tokio::test(flavor = "multi_thread")]
async fn an_unreadable_stored_commit_is_cleared_not_reinterpreted() {
    let server = TestServer::start().await;
    let mut admin = device::admin_with_group(&server).await;
    let alice = device::member_of_group(&admin, "alice").await;
    admin.wait_members(&[member(&admin), member(&alice)]).await;
    let epoch_before = admin.epoch();

    // Stage a removal of Alice with the network gone, so it is left in flight.
    let removing = {
        let user = user("alice");
        let client = &admin.client;
        async move { client.remove_user(&user).await }
    };
    let watch = async {
        let deadline = tokio::time::Instant::now() + support::device::WAIT;
        while !commit_in_flight(&admin) {
            assert!(
                tokio::time::Instant::now() < deadline,
                "no commit in flight"
            );
            tokio::time::sleep(std::time::Duration::from_millis(1)).await;
        }
        server.relay.cut();
    };
    let (result, ()) = tokio::join!(
        tokio::time::timeout(std::time::Duration::from_secs(5), removing),
        watch
    );
    drop(result);

    // Crash, then corrupt the stored record so it no longer parses, as a
    // format drift or on-disk corruption would. The staged commit stays in the
    // group, so the restart guard still sees a pending commit to recover.
    admin.client.stop();
    let path = admin.data_dir().join(client::DATABASE_FILE);
    rusqlite::Connection::open(&path)
        .and_then(|conn| {
            conn.execute(
                "UPDATE outgoing_commit SET purpose = 'not a valid record'",
                [],
            )
        })
        .check("corrupt the stored purpose");

    server.relay.restore();
    admin.client.start().await.check("start after the crash");
    admin.wait_connected().await;

    // The unreadable record is cleared, not resumed: the row is gone (the clear
    // runs in the restart guard during `start`), no rotation was merged (the
    // epoch is unchanged), and Alice is still a member because the staged
    // removal was discarded rather than resent.
    assert!(
        !commit_in_flight(&admin),
        "the unreadable commit was not cleared"
    );
    assert_eq!(
        admin.epoch(),
        epoch_before,
        "a commit was resumed (the epoch advanced) instead of being cleared"
    );
    assert!(
        admin.members().iter().any(|m| m.user == user("alice")),
        "the staged removal was resumed instead of cleared"
    );
}
