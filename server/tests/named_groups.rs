//! multi-group on the server (change `lobby-multi-group`): creating, filling
//! and closing a named group, who may moderate it, old apps kept out, and what
//! the server stores.
//!
//! Cast (support::messaging::World): the admin (`akhil`), Alice's laptop and
//! Bob's phone, all in the lobby (the system group). Only devices that
//! upload `0xF0A3` KeyPackages can join a named group.

mod support;

use support::Check;
use support::clients::TestDevice;
use support::harness::authed;
use support::messaging::{
    GROUP_ID, Stream, World, commit_envelope_in, envelope_in, reconnect, world,
};
use support::pb;
use support::pb::envelope::Kind;
use support::pb::server_frame::Frame as Out;
use support::server_external_sender;
use tonic::Code;

const NAMED: &[u8] = b"weekend-named-gr";

/// The admin's device creates the "Weekend" named group (its lobby moves to
/// `other`).
async fn create_named(w: &mut World) {
    let server = server_external_sender(&w.conn).await;
    let request = w
        .admin
        .create_named_group(NAMED, &server, "akhil@chat", true);
    w.conn
        .admin()
        .create_named_group(authed(w.admin.token(), request))
        .await
        .check("CreateNamedGroup");
}

/// A world whose Bob's phone runs a version 3 core.
async fn v3_world() -> World {
    let w = world().await;
    w.phone.upload_named_key_packages(&w.conn, 10).await;
    w
}

async fn fetch_for_named(
    w: &World,
    caller: &TestDevice,
    user: &support::clients::TestUser,
) -> Result<Vec<pb::DeviceKeyPackage>, tonic::Status> {
    w.conn
        .keys()
        .fetch_key_packages(authed(
            caller.token(),
            pb::FetchKeyPackagesRequest {
                user_id: Some(user.pb_user_id()),
                device_ids: vec![],
                group_id: NAMED.to_vec(),
            },
        ))
        .await
        .map(|r| r.into_inner().devices)
}

/// The admin adds Bob's phone to the named group; the phone joins.
async fn add_bob(w: &mut World) {
    let key_packages: Vec<Vec<u8>> = fetch_for_named(w, &w.admin, &w.bob)
        .await
        .check("fetch for the named group")
        .into_iter()
        .map(|d| d.key_package)
        .collect();
    assert_eq!(key_packages.len(), 1);
    let epoch = w.admin.epoch();
    let (commit, welcome) = w.admin.add(&key_packages);
    w.admin_stream
        .accepted(commit_envelope_in(
            NAMED,
            epoch,
            commit,
            Some((welcome, &[&w.phone])),
        ))
        .await;
    w.admin.confirm();
    let got = w.phone_stream.envelope().await;
    assert_eq!(got.kind, Kind::Welcome as i32);
    assert_eq!(got.group_id, NAMED);
    w.phone.swap_groups();
    w.phone.join(&got.ciphertext);
    w.phone_stream.ack_in(NAMED, got.server_seq).await;
}

// spec: multi-group/named-group-creation
#[tokio::test]
async fn the_admin_creates_a_named_group() {
    let mut w = v3_world().await;
    create_named(&mut w).await;
    let row: (String, i64, Option<String>, Option<i64>) = w
        .server
        .query(|db| {
            db.query_row(
                "SELECT kind, epoch, dm_user_a, closed_at FROM mls_groups WHERE group_id = ?1",
                [NAMED],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            )
        })
        .await;
    assert_eq!(row, ("named".into(), 0, None, None));
    let members: i64 = w
        .server
        .query(|db| {
            db.query_row(
                "SELECT count(*) FROM mls_members WHERE group_id = ?1",
                [NAMED],
                |r| r.get(0),
            )
        })
        .await;
    assert_eq!(members, 1);
}

// spec: multi-group/named-group-creation
#[tokio::test]
async fn only_the_admin_creates_one_and_it_needs_its_extension() {
    let mut w = v3_world().await;
    let server = server_external_sender(&w.conn).await;

    let request = w
        .phone
        .create_named_group(NAMED, &server, "akhil@chat", true);
    let status = w
        .conn
        .admin()
        .create_named_group(authed(w.phone.token(), request))
        .await
        .err()
        .check("Bob's CreateNamedGroup");
    assert_eq!(status.code(), Code::PermissionDenied);

    let request = w
        .admin
        .create_named_group(b"no-extension-grp", &server, "akhil@chat", false);
    let status = w
        .conn
        .admin()
        .create_named_group(authed(w.admin.token(), request))
        .await
        .err()
        .check("a group without 0xF0A3");
    assert_eq!(status.code(), Code::InvalidArgument);

    let request = w
        .admin
        .create_named_group(b"wrong-admin-grp!", &server, "someone@chat", true);
    let status = w
        .conn
        .admin()
        .create_named_group(authed(w.admin.token(), request))
        .await
        .err()
        .check("a group naming another admin");
    assert_eq!(status.code(), Code::InvalidArgument);

    let groups: i64 = w
        .server
        .query(|db| db.query_row("SELECT count(*) FROM mls_groups", [], |r| r.get(0)))
        .await;
    assert_eq!(groups, 1, "only the lobby");
}

// spec: group-membership/group-creation
#[tokio::test]
async fn a_named_group_is_not_a_second_system_group() {
    let mut w = v3_world().await;
    create_named(&mut w).await;
    let server = server_external_sender(&w.conn).await;
    let mut again = TestDevice::new("akhil", &w.admin_user.identity);
    let request = again.create_group(b"another-system-g", &server, "akhil@chat");
    let status = w
        .conn
        .admin()
        .create_group(authed(w.admin.token(), request))
        .await
        .err()
        .check("a second CreateGroup");
    assert_eq!(status.code(), Code::AlreadyExists);
    let systems: i64 = w
        .server
        .query(|db| {
            db.query_row(
                "SELECT count(*) FROM mls_groups WHERE kind = 'system'",
                [],
                |r| r.get(0),
            )
        })
        .await;
    assert_eq!(systems, 1);
}

// spec: multi-group/named-group-membership
#[tokio::test]
async fn the_admin_adds_and_removes_and_a_member_cannot() {
    let mut w = v3_world().await;
    w.laptop.upload_named_key_packages(&w.conn, 10).await;
    create_named(&mut w).await;
    add_bob(&mut w).await;

    // Bob's phone cannot add Alice's laptop.
    let key_packages: Vec<Vec<u8>> = fetch_for_named(&w, &w.phone, &w.alice)
        .await
        .check("Bob fetches for Alice")
        .into_iter()
        .map(|d| d.key_package)
        .collect();
    let epoch = w.phone.epoch();
    let (commit, welcome) = w.phone.add(&key_packages);
    let rejected = w
        .phone_stream
        .rejected(commit_envelope_in(
            NAMED,
            epoch,
            commit,
            Some((welcome, &[&w.laptop])),
        ))
        .await;
    assert_eq!(rejected.reason, pb::RejectionReason::PolicyViolation as i32);
    w.phone.abort();

    // The admin removes Bob's phone; it stays in the lobby.
    let leaf = w.phone.leaf();
    let epoch = w.admin.epoch();
    let commit = w.admin.remove(&[leaf]);
    w.admin_stream
        .accepted(commit_envelope_in(NAMED, epoch, commit, None))
        .await;
    w.admin.confirm();
    let (named, lobby): (i64, i64) = w
        .server
        .query(|db| {
            db.query_row(
                "SELECT (SELECT count(*) FROM mls_members WHERE group_id = ?1),
                        (SELECT count(*) FROM mls_members WHERE group_id = ?2)",
                [NAMED, GROUP_ID],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
        })
        .await;
    assert_eq!(named, 1, "the admin alone");
    assert_eq!(lobby, 3, "the admin, Alice's laptop and Bob's phone");
}

// spec: group-membership/automatic-add-of-new-users
#[tokio::test]
async fn a_new_member_is_in_the_lobby_and_no_named_group() {
    let mut w = v3_world().await;
    create_named(&mut w).await;
    let in_named: i64 = w
        .server
        .query(|db| {
            db.query_row(
                "SELECT count(*) FROM mls_members m JOIN mls_groups g ON g.group_id = m.group_id
                 WHERE g.kind = 'named' AND m.device_id != (SELECT device_id FROM mls_members
                       WHERE group_id = ?1 AND leaf_index = 0)",
                [NAMED],
                |r| r.get(0),
            )
        })
        .await;
    assert_eq!(in_named, 0);
    let in_lobby: i64 = w
        .server
        .query(|db| {
            db.query_row(
                "SELECT count(*) FROM mls_members m JOIN mls_groups g ON g.group_id = m.group_id
                 WHERE g.kind = 'system'",
                [],
                |r| r.get(0),
            )
        })
        .await;
    assert_eq!(in_lobby, 3, "the world's admin, laptop and phone");
}

// spec: multi-group/named-groups-reach-only-v3-apps
// spec: key-directory/keypackage-fetch
#[tokio::test]
async fn a_fetch_for_a_named_group_skips_old_devices_and_needs_a_member() {
    let mut w = v3_world().await;
    create_named(&mut w).await;

    let before = count_alice(&w).await;
    let bob = fetch_for_named(&w, &w.admin, &w.bob)
        .await
        .check("fetch Bob");
    assert_eq!(bob.len(), 1, "Bob's v3 phone");
    let alice = fetch_for_named(&w, &w.admin, &w.alice)
        .await
        .check("fetch Alice");
    assert!(alice.is_empty(), "Alice's laptop runs an older core");
    assert_eq!(
        count_alice(&w).await,
        before,
        "nothing of Alice's was consumed"
    );

    let status = fetch_for_named(&w, &w.phone, &w.bob)
        .await
        .err()
        .check("a non-member fetches");
    assert_eq!(status.code(), Code::PermissionDenied);
}

async fn count_alice(w: &World) -> i64 {
    w.server
        .query(|db| {
            db.query_row(
                "SELECT count(*) FROM key_packages WHERE device_id IN
                 (SELECT device_id FROM devices WHERE username = 'alice')",
                [],
                |r| r.get::<_, i64>(0),
            )
        })
        .await
}

/// The next `GroupClosed` frame; other frames before it are skipped.
async fn next_group_closed(stream: &mut Stream) -> pb::GroupClosed {
    loop {
        if let Out::GroupClosed(closed) = stream.next().await {
            return closed;
        }
    }
}

// spec: multi-group/closing-a-named-group
#[tokio::test]
async fn closing_a_named_group_is_durable_and_final() {
    let mut w = v3_world().await;
    create_named(&mut w).await;
    add_bob(&mut w).await;

    w.conn
        .admin()
        .close_group(authed(
            w.admin.token(),
            pb::CloseGroupRequest {
                group_id: NAMED.to_vec(),
            },
        ))
        .await
        .check("CloseGroup");

    let closed = next_group_closed(&mut w.phone_stream).await;
    assert_eq!(closed.group_id, NAMED);
    assert!(closed.closed_at.is_some());
    w.phone_stream = reconnect(&w.conn, &mut w.phone).await;
    let again = next_group_closed(&mut w.phone_stream).await;
    assert_eq!(again.group_id, NAMED);
    w.phone_stream.ack_in(NAMED, 0).await;
    w.phone_stream.sync().await;
    w.phone_stream = reconnect(&w.conn, &mut w.phone).await;
    w.phone_stream.assert_quiet().await;

    let epoch = w.phone.epoch();
    let ciphertext = w.phone.message(b"still there?");
    let rejected = w
        .phone_stream
        .rejected(envelope_in(NAMED, Kind::Application, epoch, ciphertext))
        .await;
    assert_eq!(rejected.reason, pb::RejectionReason::GroupClosed as i32);

    let (closed_at, members, state): (Option<i64>, i64, i64) = w
        .server
        .query(|db| {
            db.query_row(
                "SELECT closed_at,
                        (SELECT count(*) FROM mls_members WHERE group_id = ?1),
                        (SELECT count(*) FROM mls_public_state WHERE group_id = ?1)
                 FROM mls_groups WHERE group_id = ?1",
                [NAMED],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
        })
        .await;
    assert!(closed_at.is_some());
    assert_eq!((members, state), (0, 0));
}

// spec: multi-group/closing-a-named-group
// spec: multi-group/the-system-lobby
#[tokio::test]
async fn the_lobby_cannot_be_closed_and_only_the_admin_closes() {
    let mut w = v3_world().await;
    create_named(&mut w).await;
    let status = w
        .conn
        .admin()
        .close_group(authed(
            w.admin.token(),
            pb::CloseGroupRequest {
                group_id: GROUP_ID.to_vec(),
            },
        ))
        .await
        .err()
        .check("closing the lobby");
    assert_eq!(status.code(), Code::FailedPrecondition);
    let status = w
        .conn
        .admin()
        .close_group(authed(
            w.phone.token(),
            pb::CloseGroupRequest {
                group_id: NAMED.to_vec(),
            },
        ))
        .await
        .err()
        .check("Bob closes it");
    assert_eq!(status.code(), Code::PermissionDenied);
    let status = w
        .conn
        .admin()
        .close_group(authed(
            w.admin.token(),
            pb::CloseGroupRequest {
                group_id: b"no-such-group!!!".to_vec(),
            },
        ))
        .await
        .err()
        .check("closing nothing");
    assert_eq!(status.code(), Code::NotFound);
}

// spec: group-membership/membership-state-on-the-server
#[tokio::test]
async fn the_server_stores_kind_epoch_members_and_no_name() {
    let mut w = v3_world().await;
    create_named(&mut w).await;
    add_bob(&mut w).await;
    let kinds: Vec<(String, i64)> = w
        .server
        .query(|db| {
            let mut stmt = db.prepare("SELECT kind, epoch FROM mls_groups ORDER BY kind")?;
            let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?;
            rows.collect()
        })
        .await;
    assert_eq!(kinds[0].0, "named");
    assert_eq!(kinds[0].1, 1);
    assert_eq!(kinds[1].0, "system");
    let columns: Vec<String> = w
        .server
        .query(|db| {
            let mut stmt = db.prepare("SELECT name FROM pragma_table_info('mls_groups')")?;
            let rows = stmt.query_map([], |r| r.get(0))?;
            rows.collect()
        })
        .await;
    assert!(
        columns
            .iter()
            .all(|c| !c.contains("name") || c.contains("user"))
    );
}

// spec: wire-transport/protocol-version-negotiation
#[tokio::test]
async fn hello_accepts_versions_1_through_4_and_refuses_5() {
    let w = world().await;
    // Version 4 adds manual presence (change `manual-presence`).
    for version in [1, 2, 3, 4] {
        let hello = pb::ClientFrame {
            frame: Some(support::pb::client_frame::Frame::Hello(pb::Hello {
                protocol_version: version,
            })),
        };
        let mut stream = Stream::open_with(&w.conn, w.phone.token(), Some(hello))
            .await
            .check("Connect");
        stream.sync().await;
    }
    let hello = pb::ClientFrame {
        frame: Some(support::pb::client_frame::Frame::Hello(pb::Hello {
            protocol_version: 5,
        })),
    };
    match Stream::open_with(&w.conn, w.phone.token(), Some(hello)).await {
        Err(status) => assert_eq!(status.code(), Code::FailedPrecondition),
        Ok(mut stream) => {
            let status = stream.ended().await.err().check("an error status");
            assert_eq!(status.code(), Code::FailedPrecondition);
        }
    }
}

// spec: multi-group/named-groups-reach-only-v3-apps
#[tokio::test]
async fn the_server_records_which_key_packages_list_0xf0a3() {
    let w = v3_world().await;
    let flags = |user: &'static str| {
        let user = user.to_owned();
        w.server.query(move |db| {
            db.query_row(
                "SELECT sum(named_group_capable), count(*) FROM key_packages WHERE device_id IN
                 (SELECT device_id FROM devices WHERE username = ?1)",
                [user],
                |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?)),
            )
        })
    };
    let (capable, total) = flags("bob").await;
    // Ten old one-time KeyPackages stay incapable; the new ten and the new
    // last-resort one list 0xF0A3.
    assert_eq!((capable, total), (11, 20));
    let (capable, total) = flags("alice").await;
    assert_eq!(capable, 0, "Alice's older laptop lists it nowhere");
    assert!(total > 0);
}
