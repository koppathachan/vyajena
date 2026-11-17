//! client-runtime: Local message store, and the store side of Send status and
//! identity pinning.

mod support;

use std::time::Duration;

use client::keystore::{KeyName, KeyStore, MemoryKeyStore};
use client::store::{
    IncomingMessage, InsertOutcome, OutgoingMessage, PinOutcome, SCHEMA_VERSION, Store, StoreError,
};
use client::{MessageId, SendStatus};
use support::Check;

const ALICE: &str = "alice@pi";
const BOB: &str = "bob@pi";
const ALICE_LAPTOP: [u8; 16] = [0xA1; 16];
const BOB_PHONE: [u8; 16] = [0xB0; 16];

fn hi(msg_id: u8) -> OutgoingMessage<'static> {
    OutgoingMessage {
        msg_id: [msg_id; 16],
        sender_user: ALICE,
        sender_device: ALICE_LAPTOP,
        content: b"encoded Content: hi",
        created_at: support::start_time(),
    }
}

fn from_bob(server_seq: u64, msg_id: u8, content: &'static [u8]) -> IncomingMessage<'static> {
    IncomingMessage {
        server_seq,
        msg_id: [msg_id; 16],
        sender_user: BOB,
        sender_device: BOB_PHONE,
        content,
        accepted_at: support::start_time() + Duration::from_secs(server_seq),
        received_at: support::start_time() + Duration::from_secs(100 + server_seq),
        mentions_me: false,
    }
}

// spec: client-runtime/local-message-store
#[test]
fn own_message_survives_a_crash() {
    // Alice sends "hi" and her laptop crashes before Accepted arrives.
    let dir = support::temp_dir();
    let keys = MemoryKeyStore::new();
    let id = {
        let store = support::open_store(dir.path(), &keys);
        let id = store.insert_outgoing(&hi(1)).check("store before sending");
        store
            .record_sent(id, 4, b"ciphertext of hi")
            .check("record the envelope");
        id
        // crash: the store is dropped without Accepted
    };

    let store = support::open_store(dir.path(), &keys);
    let history = store.history(10).check("history");
    assert_eq!(history.len(), 1);
    assert_eq!(history[0].content, b"encoded Content: hi");
    assert_eq!(history[0].status, Some(SendStatus::Sending));
    // The core resends it with the same msg_id and the same bytes.
    let in_flight = store.outgoing_in_flight().check("in flight");
    assert_eq!(in_flight.len(), 1);
    assert_eq!(in_flight[0].id, id);
    assert_eq!(in_flight[0].msg_id, [1; 16]);
    assert_eq!(in_flight[0].sent_epoch, Some(4));
    assert_eq!(
        in_flight[0].ciphertext.as_deref(),
        Some(&b"ciphertext of hi"[..])
    );

    store
        .mark_accepted(id, 42, support::start_time() + Duration::from_secs(5))
        .check("accepted");
    let message = store.message(id).check("read").check("message");
    assert_eq!(message.status, Some(SendStatus::Accepted));
    assert_eq!(message.server_seq, Some(42));
    assert_eq!(message.ciphertext, None, "in-flight bytes dropped");
    assert!(store.outgoing_in_flight().check("in flight").is_empty());
}

// spec: client-runtime/local-message-store
#[test]
fn crash_before_ack_shows_the_message_once() {
    // Bob's core stores a message, crashes before acking, and the Pi
    // redelivers it.
    let dir = support::temp_dir();
    let keys = MemoryKeyStore::new();
    let first = {
        let store = support::open_store(dir.path(), &keys);
        store
            .insert_incoming(&from_bob(7, 9, b"encoded Content: hello"))
            .check("store before ack")
    };
    let InsertOutcome::Inserted(id) = first else {
        panic!("first delivery not inserted: {first:?}");
    };

    let store = support::open_store(dir.path(), &keys);
    let again = store
        .insert_incoming(&from_bob(7, 9, b"encoded Content: hello"))
        .check("redelivery");
    assert_eq!(again, InsertOutcome::Duplicate(id));
    assert_eq!(store.history(10).check("history").len(), 1);
    assert_eq!(store.highest_message_seq().check("highest"), Some(7));

    let message = store.message(id).check("read").check("message");
    assert!(!message.outgoing);
    assert_eq!(message.status, None);
    assert_eq!(message.sender_user, BOB);
    assert_eq!(message.sender_device, BOB_PHONE);
    assert_eq!(
        message.accepted_at,
        Some(support::start_time() + Duration::from_secs(7))
    );
}

// spec: client-runtime/local-message-store
#[test]
fn history_is_in_server_order_with_unaccepted_own_messages_last() {
    let dir = support::temp_dir();
    let keys = MemoryKeyStore::new();
    let store = support::open_store(dir.path(), &keys);

    // Received out of local insertion order (catch-up merges), own messages
    // interleaved.
    store.insert_incoming(&from_bob(3, 3, b"three")).check("3");
    let own_accepted = store.insert_outgoing(&hi(10)).check("own");
    store.insert_incoming(&from_bob(1, 1, b"one")).check("1");
    let own_sending = store.insert_outgoing(&hi(11)).check("own");
    store
        .mark_accepted(own_accepted, 2, support::start_time())
        .check("accepted at 2");

    let history = store.history(10).check("history");
    let order: Vec<Option<u64>> = history.iter().map(|m| m.server_seq).collect();
    assert_eq!(order, [Some(1), Some(2), Some(3), None]);
    assert_eq!(history[3].id, own_sending);

    // A limit keeps the newest.
    let newest = store.history(2).check("history");
    let order: Vec<Option<u64>> = newest.iter().map(|m| m.server_seq).collect();
    assert_eq!(order, [Some(3), None]);
}

// spec: client-runtime/send-status
#[test]
fn rejected_own_message_is_failed_with_its_reason() {
    let dir = support::temp_dir();
    let keys = MemoryKeyStore::new();
    let store = support::open_store(dir.path(), &keys);
    let id = store.insert_outgoing(&hi(1)).check("own");
    assert_eq!(
        store.message(id).check("read").check("message").status,
        Some(SendStatus::Sending)
    );
    store.mark_failed(id, "too large").check("failed");
    let message = store.message(id).check("read").check("message");
    assert_eq!(
        message.status,
        Some(SendStatus::Failed {
            reason: "too large".into()
        })
    );
    assert!(store.outgoing_in_flight().check("in flight").is_empty());

    // Only an own message that is still sending changes status.
    assert!(matches!(
        store.mark_accepted(id, 5, support::start_time()),
        Err(StoreError::Inconsistent(_))
    ));
    let received = match store
        .insert_incoming(&from_bob(1, 1, b"x"))
        .check("incoming")
    {
        InsertOutcome::Inserted(id) => id,
        other => panic!("{other:?}"),
    };
    assert!(store.mark_failed(received, "nope").is_err());
    assert!(store.mark_failed(MessageId(999), "nope").is_err());
}

// spec: client-runtime/local-message-store
#[test]
fn own_msg_ids_are_unique_per_device() {
    let dir = support::temp_dir();
    let keys = MemoryKeyStore::new();
    let store = support::open_store(dir.path(), &keys);
    store.insert_outgoing(&hi(1)).check("first");
    assert!(store.insert_outgoing(&hi(1)).is_err(), "same msg_id twice");
    // Another device may use the same msg_id.
    let outcome = store.insert_incoming(&from_bob(1, 1, b"x")).check("bob");
    assert!(matches!(outcome, InsertOutcome::Inserted(_)));
}

// spec: key-directory/clients-verify-the-identity-chain
#[test]
fn identity_keys_are_pinned_on_first_sight() {
    let dir = support::temp_dir();
    let keys = MemoryKeyStore::new();
    let t0 = support::start_time();
    let t1 = t0 + Duration::from_secs(60);
    {
        let store = support::open_store(dir.path(), &keys);
        assert_eq!(store.pinned_identity_key(BOB).check("none yet"), None);
        assert_eq!(
            store.pin_identity_key(BOB, &[1; 32], t0).check("pin"),
            PinOutcome::Pinned
        );
    }
    // Pins survive a restart.
    let store = support::open_store(dir.path(), &keys);
    assert_eq!(
        store.pin_identity_key(BOB, &[1; 32], t1).check("same"),
        PinOutcome::Matches
    );

    // A different key for Bob: changed, kept pending, the pinned key stays.
    assert_eq!(
        store.pin_identity_key(BOB, &[2; 32], t1).check("other"),
        PinOutcome::Changed
    );
    let pinned = store.pinned_identity_key(BOB).check("read").check("pinned");
    assert_eq!(pinned.key, [1; 32]);
    assert_eq!(pinned.pinned_at, t0);
    assert_eq!(pinned.pending.as_deref(), Some(&[2u8; 32][..]));
    // Still changed until the user accepts it.
    assert_eq!(
        store.pin_identity_key(BOB, &[2; 32], t1).check("again"),
        PinOutcome::Changed
    );

    // Accepting a key the user was not shown does nothing.
    assert!(
        !store
            .accept_identity_key(BOB, &[3; 32], t1)
            .check("accept other")
    );
    assert!(
        !store
            .accept_identity_key(ALICE, &[2; 32], t1)
            .check("accept unknown")
    );
    assert!(store.accept_identity_key(BOB, &[2; 32], t1).check("accept"));
    let pinned = store.pinned_identity_key(BOB).check("read").check("pinned");
    assert_eq!(pinned.key, [2; 32]);
    assert_eq!(pinned.pinned_at, t1);
    assert_eq!(pinned.pending, None);
    assert_eq!(
        store
            .pin_identity_key(BOB, &[2; 32], t1)
            .check("now matches"),
        PinOutcome::Matches
    );
}

// spec: client-runtime/local-message-store
#[test]
fn transaction_commits_together_or_not_at_all() {
    let dir = support::temp_dir();
    let keys = MemoryKeyStore::new();
    let store = support::open_store(dir.path(), &keys);

    let failed: Result<(), StoreError> = store.transaction(|| {
        store.insert_incoming(&from_bob(1, 1, b"one"))?;
        store.pin_identity_key(BOB, &[1; 32], support::start_time())?;
        Err(StoreError::Inconsistent("processing failed".into()))
    });
    assert!(failed.is_err());
    assert!(store.history(10).check("history").is_empty());
    assert_eq!(store.pinned_identity_key(BOB).check("pin"), None);

    store
        .transaction(|| {
            store.insert_incoming(&from_bob(1, 1, b"one"))?;
            store.pin_identity_key(BOB, &[1; 32], support::start_time())?;
            Ok::<_, StoreError>(())
        })
        .check("transaction");
    assert_eq!(store.history(10).check("history").len(), 1);
    assert!(store.pinned_identity_key(BOB).check("pin").is_some());
}

// spec: client-runtime/local-message-store
#[cfg(unix)]
#[test]
fn the_database_is_private_to_the_user() {
    use std::os::unix::fs::PermissionsExt;

    let dir = support::temp_dir();
    let keys = MemoryKeyStore::new();
    let _store = support::open_store(dir.path(), &keys);
    let mode = std::fs::metadata(dir.path().join("local.sqlite"))
        .check("database file")
        .permissions()
        .mode()
        & 0o777;
    assert_eq!(mode, 0o600);
}

// spec: client-runtime/local-message-store
#[test]
fn a_database_from_a_newer_version_is_refused() {
    let dir = support::temp_dir();
    let keys = MemoryKeyStore::new();
    let path = dir.path().join("local.sqlite");
    drop(support::open_store(dir.path(), &keys));
    let conn = rusqlite::Connection::open(&path).check("raw open");
    conn.pragma_update(None, "user_version", SCHEMA_VERSION + 1)
        .check("bump version");
    drop(conn);
    let error = Store::open(&path, &keys).err().check("open newer");
    assert!(matches!(error, StoreError::NewerSchema { .. }), "{error:?}");
}

// spec: client-runtime/key-storage
#[test]
fn copied_database_shows_messages_but_not_group_state() {
    // Someone copies Alice's SQLite file but not her key store.
    let dir = support::temp_dir();
    let keys = MemoryKeyStore::new();
    let store = support::open_store(dir.path(), &keys);
    store.insert_outgoing(&hi(1)).check("own");
    store
        .insert_incoming(&from_bob(1, 2, b"encoded Content: hello alice"))
        .check("received");
    drop(store);

    let copy = support::temp_dir();
    std::fs::copy(
        dir.path().join("local.sqlite"),
        copy.path().join("local.sqlite"),
    )
    .check("copy");
    // They can read the messages ...
    let conn = rusqlite::Connection::open(copy.path().join("local.sqlite")).check("raw open");
    let contents: Vec<Vec<u8>> = conn
        .prepare("SELECT content FROM messages ORDER BY local_id")
        .check("prepare")
        .query_map([], |row| row.get(0))
        .check("query")
        .collect::<Result<_, _>>()
        .check("rows");
    assert_eq!(contents[1], b"encoded Content: hello alice");
    drop(conn);
    // ... but with their own key store the group state does not open.
    let thief_keys = MemoryKeyStore::new();
    let error = Store::open(&copy.path().join("local.sqlite"), &thief_keys)
        .err()
        .check("open without Alice's storage key");
    assert!(error.to_string().contains("another storage key"), "{error}");
    // The storage key never went into the file.
    let storage_key = keys.get(KeyName::StorageKey).check("get").check("key");
    let bytes = support::database_bytes(&dir.path().join("local.sqlite"));
    assert!(!support::contains(&bytes, &storage_key));
}
