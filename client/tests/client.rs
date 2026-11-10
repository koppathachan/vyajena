//! The public `Client` (client-core design D2, D7): opening, the one event
//! stream, the injectable clock.

mod support;

use std::sync::Arc;
use std::time::Duration;

use client::keystore::{KeyName, KeyStore, KeyStoreKind, MemoryKeyStore};
use client::{Client, ClientConfig, Clock, UserId};
use support::Check;

// spec: client-runtime/key-storage
#[tokio::test]
async fn opening_creates_the_storage_key_in_the_key_store_once() {
    let dir = support::temp_dir();
    let keys = MemoryKeyStore::new();
    assert!(keys.get(KeyName::StorageKey).check("get").is_none());

    let client = Client::open(ClientConfig::new(dir.path(), support::shared(&keys)))
        .await
        .check("open");
    assert_eq!(client.key_store_kind(), KeyStoreKind::Memory);
    let first = keys
        .get(KeyName::StorageKey)
        .check("get")
        .check("storage key");
    assert_eq!(first.len(), 32);
    drop(client);

    // A restart reads the same key; nothing else is created.
    let _client = Client::open(ClientConfig::new(dir.path(), support::shared(&keys)))
        .await
        .check("reopen");
    let second = keys
        .get(KeyName::StorageKey)
        .check("get")
        .check("storage key");
    assert_eq!(first, second);
    assert!(keys.get(KeyName::IdentityKey).check("get").is_none());
}

// spec: client-runtime/key-storage
#[tokio::test]
async fn opening_with_another_storage_key_fails() {
    // Someone copies the data directory but not the key store.
    let dir = support::temp_dir();
    let keys = MemoryKeyStore::new();
    let client = Client::open(ClientConfig::new(dir.path(), support::shared(&keys)))
        .await
        .check("open");
    drop(client);

    let other_keys = MemoryKeyStore::new();
    let error = Client::open(ClientConfig::new(dir.path(), support::shared(&other_keys)))
        .await
        .err()
        .check("open with another key store");
    assert!(error.to_string().contains("another storage key"), "{error}");
}

// spec: client-runtime/events-in-order
#[tokio::test]
async fn there_is_one_event_stream() {
    let dir = support::temp_dir();
    let keys = MemoryKeyStore::new();
    let mut client = Client::open(ClientConfig::new(dir.path(), support::shared(&keys)))
        .await
        .check("open");
    let mut events = client.take_events().check("first take");
    assert!(client.take_events().is_none(), "a second stream");
    assert!(
        events.try_next().is_none(),
        "no events before anything happens"
    );
    drop(client);
    // The client is gone, so the stream ends.
    assert!(events.next().await.is_none());
}

// spec: group-membership/own-leaf-rotation
#[tokio::test]
async fn the_client_reads_time_from_its_clock() {
    // Rotation, renewal and inactivity are tested by moving a manual clock
    // (design D7), not by waiting.
    let dir = support::temp_dir();
    let keys = MemoryKeyStore::new();
    let clock = support::manual_clock();
    let client = Client::open(
        ClientConfig::new(dir.path(), support::shared(&keys)).with_clock(Arc::new(clock.clone())),
    )
    .await
    .check("open");
    assert_eq!(client.now(), support::start_time());
    clock.advance(Duration::from_secs(7 * 24 * 60 * 60));
    assert_eq!(
        client.now(),
        support::start_time() + Duration::from_secs(604_800)
    );

    let shared = clock.clone();
    shared.advance(Duration::from_secs(1));
    assert_eq!(clock.now(), client.now());
}

// spec: client-runtime/events-in-order
#[test]
fn user_ids_print_as_username_at_server() {
    let alice = UserId {
        username: "alice".into(),
        server_id: "pi".into(),
    };
    assert_eq!(alice.to_string(), "alice@pi");
}
