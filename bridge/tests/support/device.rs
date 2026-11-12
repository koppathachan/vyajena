//! One device driven through the bridge's `Core`, as the app drives it.

use std::sync::Arc;
use std::time::Duration;

use bridge::api::core::Core;
use bridge::api::types::{ConnectionStatus, CoreEvent, UserRef};
use client::keystore::MemoryKeyStore;
use client::{ClientConfig, ManualClock};
use parking_lot::Mutex;
use tokio::sync::Notify;

use super::pi::{ADMIN, SERVER_ID, TestPi};
use super::{Check, start_time};

/// How long a test waits for something it expects.
pub const WAIT: Duration = Duration::from_secs(20);

/// `<username>@pi`.
pub fn user(username: &str) -> UserRef {
    UserRef {
        username: username.to_owned(),
        server_id: SERVER_ID.to_owned(),
    }
}

#[derive(Default)]
struct Log {
    events: Mutex<Vec<CoreEvent>>,
    changed: Notify,
}

/// A device: the bridge's `Core` on a temp data dir, memory keys and a
/// manual clock, with its events collected in order.
pub struct Device {
    /// The core.
    pub core: Arc<Core>,
    log: Arc<Log>,
    seen: usize,
    _dir: tempfile::TempDir,
}

impl Device {
    /// A fresh device.
    pub async fn new() -> Self {
        Self::open(true).await
    }

    /// A fresh device on the system clock, for a real Pi binary (the app's
    /// integration test peer, `tests/app_peer.rs`).
    pub async fn on_system_clock() -> Self {
        Self::open(false).await
    }

    async fn open(manual_clock: bool) -> Self {
        let dir = tempfile::tempdir().check("temp dir");
        let mut config = ClientConfig::new(dir.path(), Arc::new(MemoryKeyStore::new()));
        if manual_clock {
            config = config.with_clock(Arc::new(ManualClock::new(start_time())));
        }
        let core = Arc::new(Core::open_with(config).await.check("open core"));
        let log = Arc::new(Log::default());
        let (reader, sink) = (Arc::clone(&core), Arc::clone(&log));
        tokio::spawn(async move {
            while let Some(event) = reader.next_event().await {
                sink.events.lock().push(event);
                sink.changed.notify_waiters();
            }
        });
        Self {
            core,
            log,
            seen: 0,
            _dir: dir,
        }
    }

    /// Every event so far.
    pub fn events(&self) -> Vec<CoreEvent> {
        self.log.events.lock().clone()
    }

    /// Waits for the next event (after those already matched) for which `f`
    /// holds, and returns it.
    pub async fn wait_event(&mut self, what: &str, f: impl Fn(&CoreEvent) -> bool) -> CoreEvent {
        self.wait_event_for(what, WAIT, f).await
    }

    /// [`Device::wait_event`] with its own time limit.
    pub async fn wait_event_for(
        &mut self,
        what: &str,
        limit: Duration,
        f: impl Fn(&CoreEvent) -> bool,
    ) -> CoreEvent {
        let deadline = tokio::time::Instant::now() + limit;
        loop {
            let notified = self.log.changed.notified();
            {
                let events = self.log.events.lock();
                if let Some(offset) = events[self.seen..].iter().position(&f) {
                    let index = self.seen + offset;
                    self.seen = index + 1;
                    return events[index].clone();
                }
            }
            if tokio::time::timeout_at(deadline, notified).await.is_err() {
                panic!("timed out waiting for {what}; events: {:?}", self.events());
            }
        }
    }

    /// Waits until connected.
    pub async fn wait_connected(&mut self) {
        self.wait_event("connected", |e| {
            matches!(
                e,
                CoreEvent::Connection {
                    status: ConnectionStatus::Connected
                }
            )
        })
        .await;
    }

    /// Waits until `f` holds for this device's core.
    pub async fn wait_until(&self, what: &str, f: impl Fn(&Core) -> bool) {
        self.wait_until_for(what, WAIT, f).await;
    }

    /// [`Device::wait_until`] with its own time limit.
    pub async fn wait_until_for(&self, what: &str, limit: Duration, f: impl Fn(&Core) -> bool) {
        let deadline = tokio::time::Instant::now() + limit;
        while !f(&self.core) {
            if tokio::time::Instant::now() >= deadline {
                panic!("timed out waiting for {what}; events: {:?}", self.events());
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    }
}

/// The Pi's bootstrap invite as the Pi prints it: bare base64url.
pub fn bootstrap_text(pi: &TestPi) -> String {
    client::identity::payload::to_text(&pi.bootstrap)
}

/// The admin's first device, registered from the bootstrap invite text,
/// started, with the group created.
pub async fn admin_with_group(pi: &TestPi) -> Device {
    let mut admin = Device::new().await;
    let id = admin
        .core
        .register(bootstrap_text(pi))
        .await
        .check("register admin");
    assert_eq!(id, user(ADMIN));
    admin.core.start().await.check("start admin");
    admin.wait_connected().await;
    admin.core.create_group().await.check("create group");
    admin
}

/// A new user invited by the admin through the invite text, registered,
/// started and added to the group; waits until it joined.
pub async fn member_of_group(admin: &Device, username: &str) -> Device {
    let invite = admin
        .core
        .create_invite(username.to_owned())
        .await
        .check("create invite");
    let text = invite.text.check("invite text");
    let mut device = Device::new().await;
    let id = device.core.register(text).await.check("register");
    assert_eq!(id, user(username));
    device.core.start().await.check("start");
    device.wait_connected().await;
    admin.core.add_user(user(username)).await.check("add user");
    device
        .wait_event("joined the group", |e| matches!(e, CoreEvent::JoinedGroup))
        .await;
    device
}
