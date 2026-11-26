//! device-identity: Federation-ready identifiers — the canonical parser for a
//! user id's `<username>@<server_id>` text form and its round-trip with
//! `Display`.

use client::UserId;

// spec: device-identity/federation-ready-identifiers
#[test]
fn parse_is_the_inverse_of_display() {
    let id = UserId::new("alice", "chat.example");
    assert_eq!(UserId::parse(&id.to_string()), Some(id));
}

// spec: device-identity/federation-ready-identifiers
#[test]
fn parse_splits_at_the_first_at_sign() {
    // `Display` joins with a single `@`, so parsing splits at the first one:
    // everything after it is the server id. Matches the `split_once('@')` the
    // call sites used.
    assert_eq!(
        UserId::parse("alice@a@b"),
        Some(UserId::new("alice", "a@b")),
    );
}

// spec: device-identity/federation-ready-identifiers
#[test]
fn parse_rejects_text_without_an_at_sign() {
    assert_eq!(UserId::parse("alice"), None);
    assert_eq!(UserId::parse(""), None);
}

// spec: device-identity/federation-ready-identifiers
#[test]
fn parse_accepts_empty_halves_like_the_old_call_sites() {
    // The replaced `split_once('@')` code never rejected an empty half, so the
    // canonical parser keeps that behavior (it is not a validity check).
    assert_eq!(UserId::parse("@chat"), Some(UserId::new("", "chat")));
    assert_eq!(UserId::parse("alice@"), Some(UserId::new("alice", "")));
    assert_eq!(UserId::parse("@"), Some(UserId::new("", "")));
}
