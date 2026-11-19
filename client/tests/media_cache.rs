//! The device media cache (change `media-blobs`; client-runtime: Image
//! download and cache): decrypted pictures as files outside the database, an
//! LRU cap, and the sender's own pictures kept for the blob's 7 days.

mod support;

use std::time::{Duration, SystemTime};

use client::keystore::MemoryKeyStore;
use client::media::MediaCache;
use client::store::Store;
use protocol::blob_store::BLOB_TTL;
use support::Check;

const T0: Duration = Duration::from_secs(1_800_000_000);

fn at(seconds: u64) -> SystemTime {
    SystemTime::UNIX_EPOCH + T0 + Duration::from_secs(seconds)
}

fn id(n: u8) -> [u8; 16] {
    [n; 16]
}

struct Fixture {
    _dir: tempfile::TempDir,
    store: Store,
    cache: MediaCache,
}

fn fixture(max_bytes: u64) -> Fixture {
    let dir = support::temp_dir();
    let store = support::open_store(dir.path(), &MemoryKeyStore::new());
    let cache = MediaCache::new(dir.path().join("media"), max_bytes);
    Fixture {
        _dir: dir,
        store,
        cache,
    }
}

fn files(f: &Fixture) -> usize {
    std::fs::read_dir(f.cache.dir())
        .map(|dir| dir.count())
        .unwrap_or(0)
}

// spec: client-runtime/image-download-and-cache
#[test]
fn a_cached_picture_comes_back_and_lives_outside_the_database() {
    let f = fixture(1_000_000);
    assert_eq!(f.cache.get(&f.store, &id(1), at(0)).check("get"), None);
    f.cache
        .put(&f.store, &id(1), b"picture", false, at(1))
        .check("put");
    assert!(f.cache.contains(&f.store, &id(1)).check("contains"));
    assert_eq!(
        f.cache.get(&f.store, &id(1), at(2)).check("get"),
        Some(b"picture".to_vec())
    );
    // A file in the media directory, owner-only.
    assert_eq!(files(&f), 1);
    use std::os::unix::fs::PermissionsExt;
    let mode = std::fs::metadata(f.cache.dir())
        .check("dir")
        .permissions()
        .mode();
    assert_eq!(mode & 0o777, 0o700);
}

// spec: client-runtime/image-download-and-cache
#[test]
fn the_least_recently_shown_picture_goes_first_when_over_the_cap() {
    let f = fixture(250);
    for n in 1..=2 {
        f.cache
            .put(&f.store, &id(n), &[n; 100], false, at(u64::from(n)))
            .check("put");
    }
    // Showing 1 makes 2 the oldest.
    f.cache.get(&f.store, &id(1), at(10)).check("get");
    f.cache
        .put(&f.store, &id(3), &[3; 100], false, at(11))
        .check("put");
    assert!(f.cache.contains(&f.store, &id(1)).check("1"));
    assert!(!f.cache.contains(&f.store, &id(2)).check("2"), "evicted");
    assert!(f.cache.contains(&f.store, &id(3)).check("3"));
    assert_eq!(files(&f), 2, "its file went too");
}

// spec: client-runtime/image-download-and-cache
#[test]
fn the_picture_just_cached_stays_even_if_it_alone_is_over_the_cap() {
    let f = fixture(10);
    f.cache
        .put(&f.store, &id(1), &[1; 100], false, at(0))
        .check("put");
    assert!(f.cache.contains(&f.store, &id(1)).check("contains"));
}

// spec: client-runtime/expired-media
#[test]
fn a_picture_this_device_sent_is_kept_for_the_seven_days_then_evictable() {
    let f = fixture(250);
    f.cache
        .put(&f.store, &id(1), &[1; 100], true, at(0))
        .check("put own");
    f.cache
        .put(&f.store, &id(2), &[2; 100], false, at(1))
        .check("put");
    // Over the cap: the received one goes, not the older sent one.
    f.cache
        .put(&f.store, &id(3), &[3; 100], false, at(2))
        .check("put");
    assert!(f.cache.contains(&f.store, &id(1)).check("own kept"));
    assert!(!f.cache.contains(&f.store, &id(2)).check("received evicted"));

    // After the blob's 7 days the sent one is an ordinary entry.
    let later = BLOB_TTL.as_secs() + 10;
    f.cache
        .put(&f.store, &id(4), &[4; 100], false, at(later))
        .check("put");
    assert!(
        !f.cache
            .contains(&f.store, &id(1))
            .check("own, after 7 days")
    );
}

// spec: client-runtime/image-download-and-cache
#[test]
fn a_file_that_went_missing_is_forgotten_not_an_error() {
    let f = fixture(1_000);
    f.cache
        .put(&f.store, &id(1), b"picture", false, at(0))
        .check("put");
    for entry in std::fs::read_dir(f.cache.dir()).check("dir") {
        std::fs::remove_file(entry.check("entry").path()).check("remove");
    }
    assert_eq!(f.cache.get(&f.store, &id(1), at(1)).check("get"), None);
    assert!(f.store.media_entry(&id(1)).check("entry").is_none());
}

// spec: client-runtime/expired-media
#[test]
fn an_expired_blob_is_remembered() {
    let f = fixture(1_000);
    assert!(!f.store.is_media_expired(&id(1)).check("expired"));
    f.store.mark_media_expired(&id(1), at(0)).check("mark");
    f.store
        .mark_media_expired(&id(1), at(1))
        .check("mark again");
    assert!(f.store.is_media_expired(&id(1)).check("expired"));
    assert!(!f.store.is_media_expired(&id(2)).check("other"));
}

// spec: client-runtime/image-download-and-cache
#[test]
fn putting_again_replaces_the_picture() {
    let f = fixture(1_000);
    f.cache
        .put(&f.store, &id(1), b"first", false, at(0))
        .check("put");
    f.cache
        .put(&f.store, &id(1), b"second!", true, at(1))
        .check("put");
    assert_eq!(
        f.cache.get(&f.store, &id(1), at(2)).check("get"),
        Some(b"second!".to_vec())
    );
    let entry = f.store.media_entry(&id(1)).check("entry").check("some");
    assert_eq!(entry.size, 7);
    assert!(entry.own, "once own, always own");
}
