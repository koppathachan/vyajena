//! client-runtime: Resume across restart, end to end against the real Pi: the
//! session token the core last held is kept in the key store, so `stop()` then
//! `start()` (the app going to the background and coming back) resumes without
//! `Authenticate`.

mod support;

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use client::keystore::{KeyName, KeyStore, KeyStoreError, KeyStoreKind, MemoryKeyStore, Secret};
use client::{Client, ClientConfig, ConnectionState, Event};
use protocol::wire_transport::SESSION_TOKEN_MAX_LIFETIME;
use support::Check;
use support::device::{self, user};
use support::pi::TestPi;

/// The device's clock reaches its token's renewal point: the lifetime less the
/// 5 minute margin, plus one minute.
fn past_renewal() -> Duration {
    SESSION_TOKEN_MAX_LIFETIME - Duration::from_secs(4 * 60)
}

/// How many session rows the Pi holds for `device`: one per `Authenticate`.
async fn session_count(pi: &TestPi, device: [u8; 16]) -> i64 {
    let state = pi.state.clone();
    state
        .store
        .call(move |conn| {
            Ok(conn.query_row(
                "SELECT count(*) FROM sessions WHERE device_id = ?1",
                [device.as_slice()],
                |row| row.get::<_, i64>(0),
            )?)
        })
        .await
        .check("sessions")
}

async fn restart(device: &mut device::Device) {
    device.client.stop();
    device.client.start().await.check("start");
    device.wait_connected().await;
}

fn saved(keys: &MemoryKeyStore) -> Option<Secret> {
    keys.get(KeyName::SessionToken).check("key store")
}

// spec: client-runtime/resume-across-restart
#[tokio::test(flavor = "multi_thread")]
async fn a_reopen_within_the_tokens_life_resumes_without_authenticate() {
    let pi = TestPi::start().await;
    let mut alice = device::admin(&pi).await;
    let id = alice.device_id();
    let before = session_count(&pi, id).await;
    assert!(
        saved(&alice.keys).is_some(),
        "the token is saved on connect"
    );

    // Two hours in the background: stop(), then start() on reopen.
    alice.client.stop();
    alice.clock.advance(Duration::from_secs(2 * 60 * 60));
    alice.client.start().await.check("start");
    alice.wait_connected().await;
    pi.wait_connected(&id).await;

    assert_eq!(
        session_count(&pi, id).await,
        before,
        "the restart must resume, not authenticate"
    );
}

// spec: client-runtime/resume-across-restart
#[tokio::test(flavor = "multi_thread")]
async fn a_reopen_after_the_renewal_point_runs_the_full_handshake() {
    let pi = TestPi::start().await;
    let mut alice = device::admin(&pi).await;
    let id = alice.device_id();
    let before = session_count(&pi, id).await;
    let first = saved(&alice.keys).check("saved");

    alice.client.stop();
    alice.clock.advance(past_renewal());
    alice.client.start().await.check("start");
    alice.wait_connected().await;
    pi.wait_connected(&id).await;

    assert_eq!(session_count(&pi, id).await, before + 1, "a fresh session");
    let second = saved(&alice.keys).check("saved again");
    assert_ne!(*first, *second, "the fresh token replaces the old one");
}

// spec: client-runtime/resume-across-restart
#[tokio::test(flavor = "multi_thread")]
async fn the_renewed_token_is_the_one_saved() {
    let pi = TestPi::start().await;
    let mut alice = device::admin(&pi).await;
    let id = alice.device_id();
    let first = saved(&alice.keys).check("saved");
    let before = session_count(&pi, id).await;

    // The token nears its end while connected: the core renews it. The Pi's
    // clock moves with the device's, so the renewed token is a full one.
    pi.clock.advance(past_renewal());
    alice.clock.advance(past_renewal());
    let deadline = tokio::time::Instant::now() + support::device::WAIT;
    while session_count(&pi, id).await <= before {
        assert!(tokio::time::Instant::now() < deadline, "no renewal");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let renewed = loop {
        let now = saved(&alice.keys).check("saved");
        if *now != *first {
            break now;
        }
        assert!(tokio::time::Instant::now() < deadline, "renewal not saved");
        tokio::time::sleep(Duration::from_millis(50)).await;
    };
    let after_renewal = session_count(&pi, id).await;

    restart(&mut alice).await;
    pi.wait_connected(&id).await;
    assert_eq!(
        session_count(&pi, id).await,
        after_renewal,
        "the restart resumed with the renewed token"
    );
    assert_eq!(*saved(&alice.keys).check("saved"), *renewed);
}

// spec: client-runtime/resume-across-restart
#[tokio::test(flavor = "multi_thread")]
async fn the_token_is_in_the_key_store_and_nowhere_else() {
    let pi = TestPi::start().await;
    let alice = device::admin_with_group(&pi).await;
    let value = saved(&alice.keys).check("saved");
    // version byte, two u64 times, then the token's text.
    let token = std::str::from_utf8(&value[17..])
        .check("token text")
        .to_owned();
    assert!(token.len() >= 32, "{token}");

    // Not in the database, its WAL, or any other file in the data directory.
    let mut checked = 0;
    for entry in std::fs::read_dir(alice.data_dir()).check("data dir") {
        let path = entry.check("entry").path();
        if path.is_file() {
            let bytes = std::fs::read(&path).check("read");
            assert!(
                !support::contains(&bytes, token.as_bytes()),
                "the token is in {}",
                path.display()
            );
            checked += 1;
        }
    }
    assert!(checked >= 1, "no data files to check");
}

// spec: client-runtime/resume-across-restart
// spec: client-runtime/connection-lifecycle
#[tokio::test(flavor = "multi_thread")]
async fn a_token_the_pi_no_longer_holds_falls_back_after_a_restart() {
    let pi = TestPi::start().await;
    let mut alice = device::admin(&pi).await;
    let id = alice.device_id();

    alice.client.stop();
    let state = pi.state.clone();
    state
        .store
        .call(move |conn| {
            Ok(conn.execute("DELETE FROM sessions WHERE device_id = ?1", [id.as_slice()])?)
        })
        .await
        .check("forget the session");

    alice.client.start().await.check("start");
    alice.wait_connected().await;
    pi.wait_connected(&id).await;
    assert_eq!(
        session_count(&pi, id).await,
        1,
        "the full handshake minted one fresh session"
    );
}

// spec: client-runtime/resume-across-restart
#[tokio::test(flavor = "multi_thread")]
async fn a_removed_device_deletes_its_saved_token() {
    let pi = TestPi::start().await;
    let admin = device::admin_with_group(&pi).await;
    let mut bob = device::member_of_group(&admin, "bob").await;
    assert!(saved(&bob.keys).is_some(), "Bob saved a token");

    admin
        .client
        .remove_user(&user("bob"))
        .await
        .check("remove bob");
    bob.wait_event("removed", |e| matches!(e, Event::Removed))
        .await;

    let deadline = tokio::time::Instant::now() + support::device::WAIT;
    while saved(&bob.keys).is_some() {
        assert!(
            tokio::time::Instant::now() < deadline,
            "the token was not deleted"
        );
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    // A later start does not resume: the Pi answers Authenticate with
    // PERMISSION_DENIED and Bob is told again.
    bob.client.stop();
    bob.client.start().await.check("start bob");
    bob.wait_event("removed again", |e| matches!(e, Event::Removed))
        .await;
}

/// A key store that can refuse the session token, for "Key store fails".
#[derive(Debug)]
struct Flaky {
    inner: MemoryKeyStore,
    failing: Arc<AtomicBool>,
}

impl Flaky {
    fn check(&self, name: KeyName) -> Result<(), KeyStoreError> {
        if name == KeyName::SessionToken && self.failing.load(Ordering::SeqCst) {
            return Err(KeyStoreError::Keychain("refused for the test".into()));
        }
        Ok(())
    }
}

impl KeyStore for Flaky {
    fn kind(&self) -> KeyStoreKind {
        self.inner.kind()
    }
    fn get(&self, name: KeyName) -> Result<Option<Secret>, KeyStoreError> {
        self.check(name)?;
        self.inner.get(name)
    }
    fn set(&self, name: KeyName, secret: &[u8]) -> Result<(), KeyStoreError> {
        self.check(name)?;
        self.inner.set(name, secret)
    }
    fn delete(&self, name: KeyName) -> Result<(), KeyStoreError> {
        self.check(name)?;
        self.inner.delete(name)
    }
}

// spec: client-runtime/resume-across-restart
#[tokio::test(flavor = "multi_thread")]
async fn a_key_store_that_fails_only_costs_the_full_handshake() {
    let pi = TestPi::start().await;
    let alice = device::admin(&pi).await;
    let id = alice.device_id();
    let before = session_count(&pi, id).await;
    alice.client.stop();

    // Same data and keys, but the key store refuses the session token.
    let failing = Arc::new(AtomicBool::new(true));
    let store = Flaky {
        inner: alice.keys.clone(),
        failing: Arc::clone(&failing),
    };
    let mut again = Client::open(
        ClientConfig::new(alice.data_dir(), Arc::new(store))
            .with_clock(Arc::new(alice.clock.clone())),
    )
    .await
    .check("open");
    let mut events = again.take_events().check("events");
    again.start().await.check("start with a failing key store");
    let connected = tokio::time::timeout(support::device::WAIT, async {
        while let Some(event) = events.next().await {
            if matches!(event, Event::Connection(ConnectionState::Connected)) {
                return true;
            }
        }
        false
    })
    .await
    .check("timed out");
    assert!(connected, "the core connects without the key store's help");
    pi.wait_connected(&id).await;
    assert_eq!(
        session_count(&pi, id).await,
        before + 1,
        "no token could be read, so it authenticated"
    );
}

// spec: client-runtime/resume-across-restart
#[tokio::test(flavor = "multi_thread")]
async fn a_saved_token_that_cannot_be_read_is_no_token() {
    use client::net::ResumeToken;
    let pi = TestPi::start().await;
    let mut alice = device::admin(&pi).await;
    let id = alice.device_id();

    // What the core saved reads back to the same bytes.
    let value = saved(&alice.keys).check("saved");
    let token = ResumeToken::decode(&value).check("decodes");
    assert_eq!(*token.encode(), *value);
    // Anything this version did not write is no token.
    assert!(ResumeToken::decode(&[]).is_none());
    assert!(ResumeToken::decode(&value[..17]).is_none(), "empty token");
    let mut other_version = value.to_vec();
    other_version[0] = 9;
    assert!(ResumeToken::decode(&other_version).is_none());
    let mut not_text = value.to_vec();
    *not_text.last_mut().check("last") = 0xff;
    assert!(ResumeToken::decode(&not_text).is_none());

    // A damaged entry at start is ignored: the core authenticates and connects.
    let before = session_count(&pi, id).await;
    alice.client.stop();
    alice
        .keys
        .set(KeyName::SessionToken, b"not a saved token")
        .check("damage");
    alice.client.start().await.check("start");
    alice.wait_connected().await;
    pi.wait_connected(&id).await;
    assert_eq!(session_count(&pi, id).await, before + 1);
}
