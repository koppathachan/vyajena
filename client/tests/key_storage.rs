//! client-runtime: Key storage. The key store implementations and the rule
//! that picks one (client-core design D3).

mod support;

use std::sync::Arc;

use client::keystore::{
    FileKeyStore, IOS_ACCESS_POLICY, KEY_FILE_MODE, KEYCHAIN_SERVICE, KeyFileOptIn, KeyName,
    KeyStore, KeyStoreError, KeyStoreKind, MemoryKeyStore, OsKeychain, PhoneKeyStore,
    choose_key_store, get_or_create,
};
use keyring_core::api::CredentialStoreApi;
use support::Check;

fn no_keychain() -> Result<MemoryKeyStore, KeyStoreError> {
    Err(KeyStoreError::NoKeychain {
        reason: "no Secret Service on this machine".into(),
    })
}

fn roundtrip(store: &dyn KeyStore) {
    for name in KeyName::ALL {
        assert!(store.get(name).check("get missing").is_none(), "{name}");
    }
    store.set(KeyName::IdentityKey, b"identity").check("set");
    store.set(KeyName::StorageKey, &[7u8; 32]).check("set");
    store
        .set(KeyName::IdentityKey, b"identity-2")
        .check("replace");
    assert_eq!(
        store
            .get(KeyName::IdentityKey)
            .check("get")
            .check("identity")
            .as_slice(),
        b"identity-2"
    );
    store.delete(KeyName::IdentityKey).check("delete");
    store.delete(KeyName::IdentityKey).check("delete twice");
    assert!(store.get(KeyName::IdentityKey).check("get").is_none());
    assert_eq!(
        store
            .get(KeyName::StorageKey)
            .check("get")
            .check("storage")
            .as_slice(),
        &[7u8; 32]
    );
}

// spec: client-runtime/key-storage
#[test]
fn memory_store_keeps_named_secrets() {
    let store = MemoryKeyStore::new();
    roundtrip(&store);
    assert_eq!(store.kind(), KeyStoreKind::Memory);
    // Clones share keys, so a test can restart a client with them.
    let clone = store.clone();
    assert!(clone.get(KeyName::StorageKey).check("get").is_some());
}

// spec: client-runtime/key-storage
#[test]
fn key_names_are_stable() {
    let names: Vec<&str> = KeyName::ALL.iter().map(|name| name.as_str()).collect();
    assert_eq!(
        names,
        [
            "identity-key",
            "device-auth-key",
            "mls-signature-key",
            "storage-key"
        ]
    );
    for name in KeyName::ALL {
        assert_eq!(KeyName::parse(name.as_str()), Some(name));
    }
    assert_eq!(KeyName::parse("session-token"), None);
}

// spec: client-runtime/key-storage
#[test]
fn get_or_create_makes_a_random_key_once() {
    let store = MemoryKeyStore::new();
    let first = get_or_create(&store, KeyName::StorageKey, 32).check("create");
    assert_eq!(first.len(), 32);
    assert_ne!(first.as_slice(), &[0u8; 32]);
    let again = get_or_create(&store, KeyName::StorageKey, 32).check("read");
    assert_eq!(first, again);
}

// spec: client-runtime/key-storage
#[test]
fn laptop_with_a_keychain_uses_the_keychain() {
    // The keychain is available: keys go there, even if the user also named a
    // key file, and no file is created.
    let dir = support::temp_dir();
    let opt_in = KeyFileOptIn::new(dir.path().join("keys"));
    let keychain = MemoryKeyStore::new();
    let store = choose_key_store(Ok(keychain.clone()), None).check("choose");
    assert_eq!(store.kind(), KeyStoreKind::Memory);
    let store = choose_key_store(Ok(keychain.clone()), Some(&opt_in)).check("choose");
    assert_eq!(store.kind(), KeyStoreKind::Memory);
    store.set(KeyName::IdentityKey, b"identity").check("set");
    assert!(keychain.get(KeyName::IdentityKey).check("get").is_some());
    assert!(!opt_in.path().exists(), "no key file without need");
}

// spec: client-runtime/key-storage
#[test]
fn headless_machine_without_opt_in_refuses() {
    let error = choose_key_store(no_keychain(), None)
        .err()
        .check("choose without keychain or opt-in");
    let KeyStoreError::NoKeychain { reason } = &error else {
        panic!("wrong error: {error:?}");
    };
    assert!(reason.contains("Secret Service"));
    // It tells the user how to opt in.
    let message = error.to_string();
    assert!(message.contains("opt in"), "{message}");
    assert!(message.contains("0600"), "{message}");

    // Any other keychain failure without an opt-in refuses the same way.
    let error = choose_key_store(
        Err::<MemoryKeyStore, _>(KeyStoreError::Keychain("locked".into())),
        None,
    )
    .err()
    .check("choose with a failing keychain");
    assert!(
        matches!(error, KeyStoreError::NoKeychain { .. }),
        "{error:?}"
    );
}

// spec: client-runtime/key-storage
#[cfg(unix)]
#[test]
fn headless_machine_with_opt_in_uses_a_0600_key_file() {
    use std::os::unix::fs::PermissionsExt;

    let dir = support::temp_dir();
    let opt_in = KeyFileOptIn::new(dir.path().join("profile").join("keys"));
    let store = choose_key_store(no_keychain(), Some(&opt_in)).check("choose");
    assert_eq!(store.kind(), KeyStoreKind::File);
    roundtrip(store.as_ref());

    let mode = std::fs::metadata(opt_in.path())
        .check("key file")
        .permissions()
        .mode()
        & 0o777;
    assert_eq!(mode, KEY_FILE_MODE);
    assert_eq!(KEY_FILE_MODE, 0o600);

    // The keys survive a restart.
    drop(store);
    let reopened = FileKeyStore::open(&opt_in).check("reopen");
    assert_eq!(
        reopened
            .get(KeyName::StorageKey)
            .check("get")
            .check("storage")
            .as_slice(),
        &[7u8; 32]
    );
    assert!(reopened.get(KeyName::IdentityKey).check("get").is_none());
}

// spec: client-runtime/key-storage
#[cfg(unix)]
#[test]
fn an_existing_key_file_keeps_its_keys_once_a_keychain_appears() {
    let dir = support::temp_dir();
    let opt_in = KeyFileOptIn::new(dir.path().join("keys"));
    let file = choose_key_store(no_keychain(), Some(&opt_in)).check("choose file");
    file.set(KeyName::IdentityKey, b"identity").check("set");
    drop(file);

    // Later a keychain is available: the keys stay where they are.
    let store = choose_key_store(Ok(MemoryKeyStore::new()), Some(&opt_in)).check("choose");
    assert_eq!(store.kind(), KeyStoreKind::File);
    assert!(store.get(KeyName::IdentityKey).check("get").is_some());
}

// spec: client-runtime/key-storage
#[cfg(unix)]
#[test]
fn a_key_file_others_can_read_is_refused() {
    use std::os::unix::fs::PermissionsExt;

    let dir = support::temp_dir();
    let opt_in = KeyFileOptIn::new(dir.path().join("keys"));
    let store = FileKeyStore::open(&opt_in).check("open");
    store.set(KeyName::IdentityKey, b"identity").check("set");
    drop(store);
    std::fs::set_permissions(opt_in.path(), std::fs::Permissions::from_mode(0o644)).check("chmod");

    let error = FileKeyStore::open(&opt_in)
        .err()
        .check("open a 0644 key file");
    assert!(
        matches!(error, KeyStoreError::InsecureFile { mode: 0o644, .. }),
        "{error:?}"
    );
    let error = choose_key_store(Ok(MemoryKeyStore::new()), Some(&opt_in))
        .err()
        .check("choose a 0644 key file");
    assert!(
        matches!(error, KeyStoreError::InsecureFile { .. }),
        "{error:?}"
    );
}

// spec: client-runtime/key-storage
#[cfg(unix)]
#[test]
fn a_damaged_key_file_is_refused() {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;

    let dir = support::temp_dir();
    let path = dir.path().join("keys");
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&path)
        .check("create");
    file.write_all(b"vyajena2-keys-v1\n\x0cidentity-key\x00\x00\x00\x40short")
        .check("write");
    drop(file);
    let error = FileKeyStore::open(&KeyFileOptIn::new(&path))
        .err()
        .check("open a truncated key file");
    assert!(
        matches!(error, KeyStoreError::CorruptFile { .. }),
        "{error:?}"
    );
}

// spec: client-runtime/key-storage
#[test]
#[ignore = "uses the real OS keychain; on macOS a rebuilt test binary shows a \
            keychain prompt. Run by hand: cargo test -p client --test key_storage -- --ignored"]
fn os_keychain_keeps_named_secrets() {
    let profile = format!("test-{}", std::process::id());
    let keychain = OsKeychain::probe(&profile).check("a keychain on this machine");
    assert_eq!(keychain.kind(), KeyStoreKind::OsKeychain);
    for name in KeyName::ALL {
        keychain.delete(name).check("clean up before");
    }
    roundtrip(&keychain);
    // A second handle (a restart) reads the same keys.
    let again = OsKeychain::probe(&profile).check("probe again");
    assert!(again.get(KeyName::StorageKey).check("get").is_some());
    for name in KeyName::ALL {
        keychain.delete(name).check("clean up after");
    }
    let store: Arc<dyn KeyStore> = Arc::new(again);
    assert!(store.get(KeyName::StorageKey).check("get").is_none());
}

// The phone stores (flutter-app D4) share PhoneKeyStore's entry naming and
// error mapping; here keyring-core's mock store stands in for the iOS
// Keychain and the Android Keystore, which run only on a phone (the real
// stores were checked on the iOS Simulator and the Android emulator,
// design D4).

fn mock_store() -> Arc<keyring_core::mock::Store> {
    keyring_core::mock::Store::new().check("mock store")
}

// spec: client-runtime/key-storage
#[test]
fn phone_store_keeps_named_secrets() {
    for kind in [KeyStoreKind::IosKeychain, KeyStoreKind::AndroidKeystore] {
        let store = PhoneKeyStore::with_store(mock_store(), kind, "app", &[])
            .probe()
            .check("probe");
        assert_eq!(store.kind(), kind);
        assert_eq!(store.profile(), "app");
        roundtrip(&store);
    }
}

// spec: client-runtime/key-storage
#[test]
fn phone_store_entries_are_named_by_service_profile_and_key() {
    let mock = mock_store();
    let store = PhoneKeyStore::with_store(mock.clone(), KeyStoreKind::IosKeychain, "app", &[]);
    store.set(KeyName::DeviceAuthKey, b"auth").check("set");
    // The same entry, seen from the native store: service "vyajena2",
    // user "<profile>/<key name>".
    let entry = mock
        .build(KEYCHAIN_SERVICE, "app/device-auth-key", None)
        .check("build");
    assert_eq!(entry.get_secret().check("native get"), b"auth");

    // A restart (a new handle on the same native store) reads the same keys.
    let again = PhoneKeyStore::with_store(mock.clone(), KeyStoreKind::IosKeychain, "app", &[]);
    assert_eq!(
        again
            .get(KeyName::DeviceAuthKey)
            .check("get")
            .check("auth")
            .as_slice(),
        b"auth"
    );
    // Another profile on the same phone does not share them.
    let other = PhoneKeyStore::with_store(mock, KeyStoreKind::IosKeychain, "other", &[]);
    assert!(other.get(KeyName::DeviceAuthKey).check("get").is_none());
}

// spec: client-runtime/key-storage
#[test]
fn phone_store_that_does_not_answer_is_no_keychain() {
    // The mock refuses entry modifiers, like a store that cannot build the
    // entries the core asks for: the probe reports no keychain, and without
    // an opt-in the core refuses to create keys.
    let refused = PhoneKeyStore::with_store(
        mock_store(),
        KeyStoreKind::IosKeychain,
        "app",
        &[("access-policy", IOS_ACCESS_POLICY)],
    )
    .probe();
    let error = refused.as_ref().err().check("probe must fail");
    assert!(
        matches!(error, KeyStoreError::NoKeychain { .. }),
        "{error:?}"
    );
    let chosen = choose_key_store(refused, None);
    assert!(
        matches!(chosen, Err(KeyStoreError::NoKeychain { .. })),
        "{chosen:?}"
    );
}

// spec: client-runtime/key-storage
#[test]
fn phone_store_failure_is_a_keychain_error() {
    let mock = mock_store();
    let store = PhoneKeyStore::with_store(mock.clone(), KeyStoreKind::AndroidKeystore, "app", &[]);
    store.set(KeyName::StorageKey, &[9u8; 32]).check("set");
    let entry = mock
        .build(KEYCHAIN_SERVICE, "app/storage-key", None)
        .check("build");
    let cred: &keyring_core::mock::Cred = entry.as_any().downcast_ref().check("mock cred");
    cred.set_error(keyring_core::Error::NoStorageAccess(
        "the keychain is locked".into(),
    ));
    let error = store.get(KeyName::StorageKey).err().check("get must fail");
    assert!(matches!(error, KeyStoreError::Keychain(_)), "{error:?}");
    // The error is gone after one call and the key is still there.
    assert!(store.get(KeyName::StorageKey).check("get").is_some());
}
