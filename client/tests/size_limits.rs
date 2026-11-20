//! wire-transport: Size limits, the client's half for commits: the core
//! refuses to send a `COMMIT` envelope over the 262,144-byte handshake cap,
//! counting the Welcome it carries (client-core task 7.2). The engine sends
//! every own commit through `membership::commit_envelope`; a real commit that
//! large would need hundreds of devices, so this checks the step itself.
//! Application messages: `messaging.rs` (a very long paste).

use client::ClientError;
use client::UserId;
use client::membership::{StagedOwnCommit, commit_envelope, device_ref};
use protocol::wire_transport::MAX_HANDSHAKE_ENVELOPE_BYTES;

mod support;

use support::Check;

fn staged(commit: usize, welcome: usize) -> StagedOwnCommit {
    StagedOwnCommit {
        epoch: 4,
        commit: vec![7; commit],
        welcome: Some(vec![9; welcome]),
    }
}

// spec: wire-transport/size-limits
// spec: message-delivery/welcome-travels-with-its-commit
#[test]
fn a_commit_with_a_large_welcome_is_refused_before_sending() {
    let carol = UserId::new("carol", "chat");
    let recipients = || vec![device_ref(&carol, &[3; 16])];

    // The spec's example: a 200,000-byte commit whose Welcome adds 70,000.
    let refused = commit_envelope([1; 16], &[2; 16], staged(200_000, 70_000), recipients());
    assert!(matches!(refused, Err(ClientError::TooLarge)), "{refused:?}");

    // Without the Welcome the same commit fits.
    let alone = StagedOwnCommit {
        welcome: None,
        ..staged(200_000, 0)
    };
    let encoded = commit_envelope([1; 16], &[2; 16], alone, Vec::new()).check("commit alone");
    assert!(encoded.len() <= MAX_HANDSHAKE_ENVELOPE_BYTES);

    // Right at the cap is allowed, one byte over is not.
    let overhead = commit_envelope([1; 16], &[2; 16], staged(100_000, 1_000), recipients())
        .check("small")
        .len()
        - 101_000;
    let mut welcome = MAX_HANDSHAKE_ENVELOPE_BYTES - overhead - 100_000;
    // Length prefixes grow with the size; settle on the largest that fits.
    while commit_envelope([1; 16], &[2; 16], staged(100_000, welcome), recipients()).is_err() {
        welcome -= 1;
    }
    let at_cap = commit_envelope([1; 16], &[2; 16], staged(100_000, welcome), recipients())
        .check("at the cap");
    assert_eq!(at_cap.len(), MAX_HANDSHAKE_ENVELOPE_BYTES);
    let over = commit_envelope(
        [1; 16],
        &[2; 16],
        staged(100_000, welcome + 1),
        recipients(),
    );
    assert!(matches!(over, Err(ClientError::TooLarge)), "{over:?}");
}
