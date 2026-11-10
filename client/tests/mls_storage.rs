//! client-runtime: Key storage. OpenMLS group state lives in SQLite only
//! encrypted under the storage key (client-core design D3, task 1.1).

mod support;

use client::keystore::{KeyName, KeyStore, MemoryKeyStore};
use client::store::StoreError;
use openmls_traits::OpenMlsProvider as _;
use support::Check;
use support::mls;

// spec: client-runtime/key-storage
#[test]
fn group_survives_a_restart_mid_conversation() {
    let alice_dir = support::temp_dir();
    let bob_dir = support::temp_dir();
    let alice_keys = MemoryKeyStore::new();
    let bob_keys = MemoryKeyStore::new();
    let alice_signer = mls::signer();
    let bob_signer = mls::signer();

    let (group_id, in_flight) = {
        let alice = support::open_store(alice_dir.path(), &alice_keys);
        let bob = support::open_store(bob_dir.path(), &bob_keys);
        let key_package = mls::key_package(&bob, "bob", &bob_signer);
        let (mut alice_group, welcome) = mls::create_and_add(&alice, &alice_signer, key_package);
        let mut bob_group = mls::join(&bob, &welcome);

        let hi = mls::send(&alice, &mut alice_group, &alice_signer, b"hi bob");
        assert_eq!(
            mls::receive(&bob, &mut bob_group, &hi).check("read"),
            Some(b"hi bob".to_vec())
        );
        // A second message is on its way when both devices restart.
        let in_flight = mls::send(&alice, &mut alice_group, &alice_signer, b"still there?");
        (alice_group.group_id().clone(), in_flight)
    };

    let alice = support::open_store(alice_dir.path(), &alice_keys);
    let bob = support::open_store(bob_dir.path(), &bob_keys);
    let mut alice_group = mls::load(&alice, &group_id);
    let mut bob_group = mls::load(&bob, &group_id);
    assert_eq!(alice_group.epoch(), bob_group.epoch());
    assert_eq!(
        mls::receive(&bob, &mut bob_group, &in_flight).check("read after restart"),
        Some(b"still there?".to_vec())
    );
    let yes = mls::send(&bob, &mut bob_group, &bob_signer, b"yes");
    assert_eq!(
        mls::receive(&alice, &mut alice_group, &yes).check("read"),
        Some(b"yes".to_vec())
    );

    // A commit after the restart, then another restart.
    let commit = mls::self_update(&bob, &mut bob_group, &bob_signer);
    assert_eq!(
        mls::receive(&alice, &mut alice_group, &commit).check("commit"),
        None
    );
    assert_eq!(alice_group.epoch(), bob_group.epoch());
    drop((bob_group, bob));
    let bob = support::open_store(bob_dir.path(), &bob_keys);
    let mut bob_group = mls::load(&bob, &group_id);
    let new_epoch = mls::send(&alice, &mut alice_group, &alice_signer, b"new epoch");
    assert_eq!(
        mls::receive(&bob, &mut bob_group, &new_epoch).check("read"),
        Some(b"new epoch".to_vec())
    );
}

// spec: client-runtime/key-storage
#[test]
fn key_package_made_before_a_restart_can_be_joined_after_it() {
    let alice_dir = support::temp_dir();
    let bob_dir = support::temp_dir();
    let alice_keys = MemoryKeyStore::new();
    let bob_keys = MemoryKeyStore::new();
    let alice_signer = mls::signer();
    let bob_signer = mls::signer();

    let key_package = {
        let bob = support::open_store(bob_dir.path(), &bob_keys);
        mls::key_package(&bob, "bob", &bob_signer)
    };
    let alice = support::open_store(alice_dir.path(), &alice_keys);
    let (mut alice_group, welcome) = mls::create_and_add(&alice, &alice_signer, key_package);
    let bob = support::open_store(bob_dir.path(), &bob_keys);
    let mut bob_group = mls::join(&bob, &welcome);
    let hi = mls::send(&alice, &mut alice_group, &alice_signer, b"welcome");
    assert_eq!(
        mls::receive(&bob, &mut bob_group, &hi).check("read"),
        Some(b"welcome".to_vec())
    );
}

// spec: client-runtime/key-storage
#[test]
fn copied_database_cannot_decrypt_new_group_traffic() {
    let alice_dir = support::temp_dir();
    let bob_dir = support::temp_dir();
    let alice_keys = MemoryKeyStore::new();
    let bob_keys = MemoryKeyStore::new();
    let alice_signer = mls::signer();
    let bob_signer = mls::signer();
    let alice = support::open_store(alice_dir.path(), &alice_keys);
    let bob = support::open_store(bob_dir.path(), &bob_keys);
    let key_package = mls::key_package(&bob, "bob", &bob_signer);
    let (mut alice_group, welcome) = mls::create_and_add(&alice, &alice_signer, key_package);
    let bob_group = mls::join(&bob, &welcome);
    let group_id = bob_group.group_id().clone();
    drop((bob_group, bob));

    // The file holds no label, group id or credential in the clear ...
    let bytes = support::database_bytes(&bob_dir.path().join("local.sqlite"));
    for needle in [
        &b"GroupState"[..],
        b"EpochSecrets",
        b"bob",
        group_id.as_slice(),
    ] {
        assert!(
            !support::contains(&bytes, needle),
            "{} in the clear",
            String::from_utf8_lossy(needle)
        );
    }
    let storage_key = bob_keys.get(KeyName::StorageKey).check("get").check("key");
    assert!(!support::contains(&bytes, &storage_key));

    // ... and a copy opened with another key store does not open at all.
    let copy = support::temp_dir();
    std::fs::copy(
        bob_dir.path().join("local.sqlite"),
        copy.path().join("local.sqlite"),
    )
    .check("copy");
    let error =
        client::store::Store::open(&copy.path().join("local.sqlite"), &MemoryKeyStore::new())
            .err()
            .check("open the copy");
    assert!(matches!(error, StoreError::Mls(_)), "{error:?}");

    // Bob, with his key store, still reads new traffic.
    let bob = support::open_store(bob_dir.path(), &bob_keys);
    let mut bob_group = mls::load(&bob, &group_id);
    let message = mls::send(&alice, &mut alice_group, &alice_signer, b"new traffic");
    assert_eq!(
        mls::receive(&bob, &mut bob_group, &message).check("read"),
        Some(b"new traffic".to_vec())
    );
}

// spec: client-runtime/key-storage
#[test]
fn tampered_group_state_does_not_load() {
    let dir = support::temp_dir();
    let keys = MemoryKeyStore::new();
    let signer = mls::signer();
    let other = mls::signer();
    let path = dir.path().join("local.sqlite");
    let group_id = {
        let store = support::open_store(dir.path(), &keys);
        let key_package = mls::key_package(&store, "self", &other);
        let (group, _) = mls::create_and_add(&store, &signer, key_package);
        group.group_id().clone()
    };
    // Move every sealed value to the next row's slot.
    let conn = rusqlite::Connection::open(&path).check("raw open");
    let rows: Vec<(Vec<u8>, Vec<u8>)> = conn
        .prepare("SELECT slot, value FROM mls_storage WHERE length(slot) = 32")
        .check("prepare")
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
        .check("query")
        .collect::<Result<_, _>>()
        .check("rows");
    assert!(rows.len() > 2);
    for (index, (slot, _)) in rows.iter().enumerate() {
        let value = &rows[(index + 1) % rows.len()].1;
        conn.execute(
            "UPDATE mls_storage SET value = ?1 WHERE slot = ?2",
            rusqlite::params![value, slot],
        )
        .check("swap");
    }
    drop(conn);
    let store = support::open_store(dir.path(), &keys);
    assert!(openmls::prelude::MlsGroup::load(store.mls().storage(), &group_id).is_err());
}

// spec: client-runtime/key-storage
#[test]
fn signature_keys_are_never_written_to_the_database() {
    let dir = support::temp_dir();
    let keys = MemoryKeyStore::new();
    let store = support::open_store(dir.path(), &keys);
    let signer = mls::signer();
    assert!(signer.store(store.mls().storage()).is_err());
    assert!(
        openmls_basic_credential::SignatureKeyPair::read(
            store.mls().storage(),
            signer.public(),
            mls::CIPHERSUITE.signature_algorithm()
        )
        .is_none()
    );
}

// spec: client-runtime/local-message-store
#[test]
fn decrypt_and_store_roll_back_together() {
    // Receiving decrypts and stores in one transaction, so a failure before
    // the commit leaves the message decryptable again after a reload; after
    // the commit its key is used up.
    let alice_dir = support::temp_dir();
    let bob_dir = support::temp_dir();
    let alice_keys = MemoryKeyStore::new();
    let bob_keys = MemoryKeyStore::new();
    let alice_signer = mls::signer();
    let bob_signer = mls::signer();
    let alice = support::open_store(alice_dir.path(), &alice_keys);
    let bob = support::open_store(bob_dir.path(), &bob_keys);
    let key_package = mls::key_package(&bob, "bob", &bob_signer);
    let (mut alice_group, welcome) = mls::create_and_add(&alice, &alice_signer, key_package);
    let mut bob_group = mls::join(&bob, &welcome);
    let group_id = bob_group.group_id().clone();
    let message = mls::send(&alice, &mut alice_group, &alice_signer, b"once");

    let failed: Result<(), StoreError> = bob.transaction(|| {
        let plaintext = mls::receive(&bob, &mut bob_group, &message).check("decrypt");
        assert_eq!(plaintext, Some(b"once".to_vec()));
        Err(StoreError::Inconsistent("crash before storing".into()))
    });
    assert!(failed.is_err());

    let mut bob_group = mls::load(&bob, &group_id);
    bob.transaction(|| {
        let plaintext = mls::receive(&bob, &mut bob_group, &message).check("decrypt again");
        assert_eq!(plaintext, Some(b"once".to_vec()));
        Ok::<_, StoreError>(())
    })
    .check("commit");

    let mut reloaded = mls::load(&bob, &group_id);
    assert!(
        mls::receive(&bob, &mut reloaded, &message).is_err(),
        "key used up"
    );
}
