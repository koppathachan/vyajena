//! blob-store: the filesystem backend on its own (change `media-blobs`).

mod support;

use std::os::unix::fs::PermissionsExt;

use server::blobs::{BlobId, BlobStore, FilesystemStore};
use support::Check;

const ID: BlobId = [7; 16];

fn store() -> (tempfile::TempDir, FilesystemStore) {
    let dir = tempfile::tempdir().check("temp dir");
    let store = FilesystemStore::new(dir.path().to_path_buf());
    (dir, store)
}

// spec: blob-store/pluggable-storage-backend
#[tokio::test]
async fn bytes_go_in_and_come_out_by_id() {
    let (_dir, store) = store();
    assert!(!store.exists(&ID).await.check("exists"));
    assert_eq!(store.get(&ID).await.check("get"), None);

    store.put(&ID, b"ciphertext".to_vec()).await.check("put");
    assert!(store.exists(&ID).await.check("exists"));
    assert_eq!(
        store.get(&ID).await.check("get"),
        Some(b"ciphertext".to_vec())
    );

    // Putting again replaces; deleting twice is fine.
    store.put(&ID, b"other".to_vec()).await.check("put");
    assert_eq!(store.get(&ID).await.check("get"), Some(b"other".to_vec()));
    store.delete(&ID).await.check("delete");
    store.delete(&ID).await.check("delete again");
    assert!(!store.exists(&ID).await.check("exists"));
}

// spec: blob-store/blobs-are-opaque-ciphertext
#[tokio::test]
async fn a_put_leaves_one_private_file_and_no_temporary_one() {
    let (dir, store) = store();
    store.put(&ID, vec![1; 100]).await.check("put");
    let files: Vec<_> = std::fs::read_dir(dir.path())
        .check("read_dir")
        .map(|entry| entry.check("entry"))
        .collect();
    assert_eq!(files.len(), 1, "only the blob");
    let mode = files[0].metadata().check("metadata").permissions().mode();
    assert_eq!(mode & 0o777, 0o600);
}

// spec: blob-store/blob-storage-and-graceful-degradation
#[tokio::test]
async fn a_missing_directory_is_unavailable_and_is_never_created() {
    let dir = tempfile::tempdir().check("temp dir");
    let gone = dir.path().join("not-mounted");
    let store = FilesystemStore::new(gone.clone());
    assert!(store.check().await.is_err());
    assert!(store.put(&ID, vec![1]).await.is_err());
    assert!(!gone.exists(), "the store never makes the directory");
}

// spec: blob-store/blob-storage-and-graceful-degradation
#[tokio::test]
async fn a_file_where_the_directory_should_be_is_unavailable() {
    let dir = tempfile::tempdir().check("temp dir");
    let file = dir.path().join("a-file");
    std::fs::write(&file, b"x").check("write");
    assert!(FilesystemStore::new(file).check().await.is_err());
}

// spec: blob-store/blob-storage-and-graceful-degradation
#[tokio::test]
async fn a_directory_the_service_cannot_write_is_unavailable() {
    let (dir, store) = store();
    std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o500)).check("chmod");
    // Root ignores the mode, so only check where the mode is honoured.
    let honoured = std::fs::write(dir.path().join("probe"), b"x").is_err();
    let result = store.check().await;
    std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o700)).check("chmod");
    if honoured {
        assert!(result.is_err());
    }
}
