//! The server's store migration for DMs (change `dms`, design D8): a database
//! from before it keeps its one group, members, public state, envelopes,
//! gaps and pending server Removes, now as the main group.

use rusqlite::{Connection, params};

const BEFORE: &[&str] = &[
    include_str!("../src/store/migrations/0001_runtime.sql"),
    include_str!("../src/store/migrations/0002_devices_keys_group.sql"),
    include_str!("../src/store/migrations/0003_messaging.sql"),
    include_str!("../src/store/migrations/0004_join_notices.sql"),
];
const GROUPS: &str = include_str!("../src/store/migrations/0005_groups.sql");

const GROUP: &[u8] = b"the-one-group-id";
const PHONE: [u8; 16] = [1; 16];

fn version_4_database() -> Connection {
    let conn = Connection::open_in_memory().expect("in-memory database");
    for sql in BEFORE {
        conn.execute_batch(sql).expect("an old migration");
    }
    conn.execute_batch(
        "INSERT INTO users (username, identity_public_key, created_at) VALUES ('bob', x'00', 1);",
    )
    .expect("a user");
    conn.execute(
        "INSERT INTO devices (device_id, username, auth_public_key, mls_signature_public_key,
                              certificate, certificate_signature, created_at)
         VALUES (?1, 'bob', x'01', x'02', x'03', x'04', 1)",
        params![PHONE.as_slice()],
    )
    .expect("a device");
    conn.execute(
        "INSERT INTO mls_group (id, group_id, epoch, pi_sender_index, created_at)
         VALUES (1, ?1, 7, 0, 1)",
        params![GROUP],
    )
    .expect("the group");
    conn.execute_batch(
        "INSERT INTO mls_public_state (key, value) VALUES (x'aa', x'bb');
         INSERT INTO envelopes (server_seq, msg_id, kind, epoch, ciphertext, accepted_at)
         VALUES (5, x'00000000000000000000000000000000', 1, 7, x'cc', 2);",
    )
    .expect("state and an envelope");
    conn.execute(
        "INSERT INTO mls_members (leaf_index, device_id) VALUES (3, ?1)",
        params![PHONE.as_slice()],
    )
    .expect("a member");
    conn.execute(
        "INSERT INTO cursors (device_id, acked_seq, gap_expired_at) VALUES (?1, 4, 99)",
        params![PHONE.as_slice()],
    )
    .expect("a cursor with a gap");
    conn.execute(
        "INSERT INTO inactive_devices (device_id, since, missed_since, proposed_epoch)
         VALUES (?1, 10, 9, 7)",
        params![PHONE.as_slice()],
    )
    .expect("an inactive device");
    conn
}

// spec: group-membership/membership-state-on-the-server
#[test]
fn a_pre_dm_database_keeps_everything_as_the_main_group() {
    let conn = version_4_database();
    conn.execute_batch(GROUPS).expect("the DM migration");

    let group: (Vec<u8>, String, i64) = conn
        .query_row("SELECT group_id, kind, epoch FROM mls_groups", [], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?))
        })
        .expect("the main group");
    assert_eq!(group, (GROUP.to_vec(), "main".into(), 7));
    let member: (Vec<u8>, i64) = conn
        .query_row("SELECT group_id, leaf_index FROM mls_members", [], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
        .expect("the member");
    assert_eq!(member, (GROUP.to_vec(), 3));
    let state: Vec<u8> = conn
        .query_row("SELECT group_id FROM mls_public_state", [], |r| r.get(0))
        .expect("the public state");
    assert_eq!(state, GROUP);
    let envelope: Vec<u8> = conn
        .query_row("SELECT group_id FROM envelopes", [], |r| r.get(0))
        .expect("the envelope");
    assert_eq!(envelope, GROUP);
    let gap: (Vec<u8>, i64) = conn
        .query_row("SELECT group_id, newest_expired_at FROM gaps", [], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
        .expect("the gap");
    assert_eq!(gap, (GROUP.to_vec(), 99));
    let removal: (Vec<u8>, i64) = conn
        .query_row("SELECT group_id, proposed_epoch FROM pi_removes", [], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
        .expect("the pending server Remove");
    assert_eq!(removal, (GROUP.to_vec(), 7));
    let cursor: i64 = conn
        .query_row("SELECT acked_seq FROM cursors", [], |r| r.get(0))
        .expect("the cursor");
    assert_eq!(cursor, 4);
}

// spec: group-membership/group-creation
#[test]
fn the_server_holds_at_most_one_main_group_and_one_open_dm_per_pair() {
    let conn = version_4_database();
    conn.execute_batch(GROUPS).expect("the DM migration");
    let second_main = conn.execute(
        "INSERT INTO mls_groups (group_id, kind, epoch, pi_sender_index, created_at)
         VALUES (x'02', 'main', 0, 0, 1)",
        [],
    );
    assert!(second_main.is_err());
    conn.execute(
        "INSERT INTO mls_groups (group_id, kind, epoch, pi_sender_index, created_at,
                                 dm_user_a, dm_user_b)
         VALUES (x'03', 'dm', 0, 0, 1, 'alice', 'bob')",
        [],
    )
    .expect("a DM");
    let second_open = conn.execute(
        "INSERT INTO mls_groups (group_id, kind, epoch, pi_sender_index, created_at,
                                 dm_user_a, dm_user_b)
         VALUES (x'04', 'dm', 0, 0, 1, 'alice', 'bob')",
        [],
    );
    assert!(second_open.is_err());
    conn.execute(
        "UPDATE mls_groups SET closed_at = 5 WHERE group_id = x'03'",
        [],
    )
    .expect("close it");
    conn.execute(
        "INSERT INTO mls_groups (group_id, kind, epoch, pi_sender_index, created_at,
                                 dm_user_a, dm_user_b)
         VALUES (x'04', 'dm', 0, 0, 1, 'alice', 'bob')",
        [],
    )
    .expect("a new DM once the old one is closed");
}

const NAMED_GROUPS: &str = include_str!("../src/store/migrations/0007_named_groups.sql");

// spec: group-membership/membership-state-on-the-server
#[test]
fn a_dm_era_database_keeps_its_group_as_the_system_lobby() {
    let conn = version_4_database();
    conn.execute_batch(GROUPS).expect("the DM migration");
    conn.execute(
        "INSERT INTO mls_groups (group_id, kind, epoch, pi_sender_index, created_at,
                                 dm_user_a, dm_user_b)
         VALUES (x'dd', 'dm', 0, 0, 2, 'alice', 'bob')",
        [],
    )
    .expect("a DM");
    // The runner turns foreign keys off around a migration that rebuilds a table.
    conn.execute_batch("PRAGMA foreign_keys = OFF")
        .expect("pragma");
    conn.execute_batch(NAMED_GROUPS)
        .expect("the named-groups migration");
    conn.execute_batch("PRAGMA foreign_keys = ON")
        .expect("pragma");
    let broken: i64 = conn
        .query_row("SELECT count(*) FROM pragma_foreign_key_check", [], |r| {
            r.get(0)
        })
        .expect("foreign_key_check");
    assert_eq!(broken, 0);

    let kinds: Vec<(Vec<u8>, String, i64)> = conn
        .prepare("SELECT group_id, kind, epoch FROM mls_groups ORDER BY created_at")
        .expect("prepare")
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
        .expect("query")
        .collect::<Result<_, _>>()
        .expect("rows");
    assert_eq!(
        kinds,
        vec![
            (GROUP.to_vec(), "system".to_string(), 7),
            (vec![0xdd], "dm".to_string(), 0)
        ]
    );
    let state: i64 = conn
        .query_row("SELECT count(*) FROM mls_public_state", [], |r| r.get(0))
        .expect("state kept");
    assert_eq!(state, 1);
    // A named group fits; a second system group and a named group with DM users do not.
    conn.execute(
        "INSERT INTO mls_groups (group_id, kind, epoch, pi_sender_index, created_at)
         VALUES (x'ee', 'named', 0, 0, 3)",
        [],
    )
    .expect("a named group");
    assert!(
        conn.execute(
            "INSERT INTO mls_groups (group_id, kind, epoch, pi_sender_index, created_at)
             VALUES (x'ff', 'system', 0, 0, 3)",
            [],
        )
        .is_err()
    );
    let flagged: i64 = conn
        .query_row(
            "SELECT count(*) FROM pragma_table_info('key_packages')
             WHERE name = 'named_group_capable'",
            [],
            |r| r.get(0),
        )
        .expect("column");
    assert_eq!(flagged, 1);
}
