//! pi-runtime: First start.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

use server::runtime::server_keys::{EXTERNAL_SENDER_KEY_FILE, TLS_KEY_FILE};
use server::runtime::{KeyError, ServerKeys};

fn mode(path: &Path) -> u32 {
    fs::metadata(path)
        .map(|m| m.permissions().mode() & 0o777)
        .unwrap_or(0)
}

fn temp() -> tempfile::TempDir {
    match tempfile::tempdir() {
        Ok(dir) => dir,
        Err(error) => panic!("no temp dir: {error}"),
    }
}

fn load(dir: &Path) -> ServerKeys {
    match ServerKeys::load_or_create(dir) {
        Ok(keys) => keys,
        Err(error) => panic!("keys refused: {error}"),
    }
}

// spec: pi-runtime/first-start
#[test]
fn fresh_pi_creates_both_key_files_with_mode_0600() {
    let tmp = temp();
    let dir = tmp.path().join("keys");
    let keys = load(&dir);

    assert!(keys.created());
    assert_eq!(mode(&dir.join(TLS_KEY_FILE)), 0o600);
    assert_eq!(mode(&dir.join(EXTERNAL_SENDER_KEY_FILE)), 0o600);
    assert_eq!(mode(&dir), 0o700);
    assert_eq!(keys.tls_spki_sha256_hex().len(), 64);
}

// spec: pi-runtime/first-start
#[test]
fn restart_keeps_the_pin_and_the_external_sender_key() {
    let tmp = temp();
    let first = load(tmp.path());
    let second = load(tmp.path());

    assert!(!second.created());
    assert_eq!(first.tls_spki_sha256(), second.tls_spki_sha256());
    assert_eq!(
        first.external_sender_public_key(),
        second.external_sender_public_key()
    );
}

// spec: pi-runtime/first-start
#[test]
fn two_fresh_pis_get_different_keys() {
    let (a, b) = (temp(), temp());
    let (a, b) = (load(a.path()), load(b.path()));
    assert_ne!(a.tls_spki_sha256(), b.tls_spki_sha256());
    assert_ne!(
        a.external_sender_public_key(),
        b.external_sender_public_key()
    );
}

// spec: pi-runtime/first-start
#[test]
fn half_a_key_folder_is_refused_not_regenerated() {
    for lost in [TLS_KEY_FILE, EXTERNAL_SENDER_KEY_FILE] {
        let tmp = temp();
        let first = load(tmp.path());
        drop(first);
        fs::remove_file(tmp.path().join(lost)).ok();

        let result = ServerKeys::load_or_create(tmp.path());
        assert!(
            matches!(result, Err(KeyError::Incomplete { .. })),
            "{lost}: {result:?}"
        );
        assert!(!tmp.path().join(lost).exists(), "{lost} was regenerated");
    }
}

// spec: pi-runtime/first-start
#[test]
fn key_readable_by_others_is_refused() {
    let tmp = temp();
    drop(load(tmp.path()));
    let path = tmp.path().join(TLS_KEY_FILE);
    fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).ok();

    let result = ServerKeys::load_or_create(tmp.path());
    assert!(
        matches!(result, Err(KeyError::Permissions { mode: 0o644, .. })),
        "{result:?}"
    );
}

// spec: pi-runtime/first-start
#[test]
fn corrupt_key_file_is_refused() {
    let tmp = temp();
    drop(load(tmp.path()));
    let path = tmp.path().join(EXTERNAL_SENDER_KEY_FILE);
    fs::write(&path, "not a key").ok();

    let result = ServerKeys::load_or_create(tmp.path());
    assert!(
        matches!(result, Err(KeyError::Invalid { .. })),
        "{result:?}"
    );
}
