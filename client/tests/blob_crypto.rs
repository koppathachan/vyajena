//! Blob crypto (change `media-blobs`; client-runtime: Blob crypto and send
//! order): a fresh key and id per image, XChaCha20-Poly1305 with the id as
//! additional authenticated data, and a nonce derived so a retried upload is
//! byte for byte the same.

use client::media::crypto::{self, CryptoError, decrypt, encrypt};
use protocol::blob_store::{BLOB_ID_LEN, BLOB_KEY_LEN, BLOB_NONCE_LEN, BLOB_TAG_LEN};

// spec: client-runtime/blob-crypto-and-send-order
#[test]
fn every_image_gets_a_fresh_key_and_id() {
    let (id1, key1) = crypto::fresh().expect("fresh");
    let (id2, key2) = crypto::fresh().expect("fresh");
    assert_eq!((id1.len(), key1.len()), (BLOB_ID_LEN, BLOB_KEY_LEN));
    assert_ne!(id1, id2, "no dedup: never the same id");
    assert_ne!(key1, key2, "never the same key");
}

// spec: client-runtime/blob-crypto-and-send-order
#[test]
fn a_blob_opens_with_its_key_and_id_and_is_the_size_the_limit_says() {
    let (id, key) = crypto::fresh().expect("fresh");
    let webp = vec![7u8; 1234];
    let blob = encrypt(&key, &id, &webp).expect("encrypt");
    assert_eq!(blob.len(), BLOB_NONCE_LEN + webp.len() + BLOB_TAG_LEN);
    assert_ne!(
        &blob[BLOB_NONCE_LEN..BLOB_NONCE_LEN + webp.len()],
        &webp[..]
    );
    assert_eq!(decrypt(&key, &id, &blob).expect("decrypt"), webp);
}

// spec: client-runtime/blob-crypto-and-send-order
#[test]
fn a_retried_encryption_is_byte_identical_so_an_upload_is_idempotent() {
    let (id, key) = crypto::fresh().expect("fresh");
    let webp = vec![9u8; 500];
    assert_eq!(
        encrypt(&key, &id, &webp).expect("encrypt"),
        encrypt(&key, &id, &webp).expect("encrypt again")
    );
    // Another key or id never gives the same nonce.
    let (other_id, other_key) = crypto::fresh().expect("fresh");
    let first = encrypt(&key, &id, &webp).expect("encrypt");
    let second = encrypt(&other_key, &id, &webp).expect("encrypt");
    let third = encrypt(&key, &other_id, &webp).expect("encrypt");
    assert_ne!(first[..BLOB_NONCE_LEN], second[..BLOB_NONCE_LEN]);
    assert_ne!(first[..BLOB_NONCE_LEN], third[..BLOB_NONCE_LEN]);
}

// spec: client-runtime/blob-crypto-and-send-order
#[test]
fn a_blob_under_the_wrong_id_key_or_damaged_does_not_open() {
    let (id, key) = crypto::fresh().expect("fresh");
    let blob = encrypt(&key, &id, b"a picture").expect("encrypt");

    // Served under another id: the id is authenticated.
    let mut other_id = id;
    other_id[0] ^= 1;
    assert_eq!(decrypt(&key, &other_id, &blob), Err(CryptoError::Invalid));
    // Another key.
    let mut other_key = key;
    other_key[0] ^= 1;
    assert_eq!(decrypt(&other_key, &id, &blob), Err(CryptoError::Invalid));
    // One flipped bit anywhere.
    for at in [0, BLOB_NONCE_LEN, blob.len() - 1] {
        let mut damaged = blob.clone();
        damaged[at] ^= 1;
        assert_eq!(
            decrypt(&key, &id, &damaged),
            Err(CryptoError::Invalid),
            "bit at {at}"
        );
    }
    // Too short to hold a nonce and a tag.
    assert_eq!(decrypt(&key, &id, &blob[..10]), Err(CryptoError::Invalid));
    assert_eq!(decrypt(&key, &id, &[]), Err(CryptoError::Invalid));
}

// spec: message-content/image-messages-stay-encrypted
#[test]
fn the_blob_does_not_contain_the_picture() {
    let (id, key) = crypto::fresh().expect("fresh");
    let webp = b"RIFF....WEBPVP8 recognisable picture bytes".repeat(20);
    let blob = encrypt(&key, &id, &webp).expect("encrypt");
    assert!(!blob.windows(16).any(|w| w == &webp[..16]));
    assert!(!blob.windows(key.len()).any(|w| w == key));
}
