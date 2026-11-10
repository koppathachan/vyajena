//! Real `Client`s for end-to-end tests: a temporary data directory, a memory
//! key store (kept across a "restart"), a manual clock, and an event log the
//! test can wait on.

use std::sync::Arc;
use std::time::Duration;

use client::keystore::MemoryKeyStore;
use client::{Client, ClientConfig, ConnectionState, Event, ManualClock, Member, UserId};
use parking_lot::Mutex;
use tokio::sync::Notify;

use super::pi::{ADMIN, SERVER_ID, TestPi};
use super::{Check, manual_clock};

/// How long a test waits for something it expects.
pub const WAIT: Duration = Duration::from_secs(20);

/// How long a test waits to be sure something does not happen.
pub const QUIET: Duration = Duration::from_millis(800);

/// `<username>@pi`.
pub fn user(username: &str) -> UserId {
    UserId::new(username, SERVER_ID)
}

#[derive(Default)]
struct Log {
    events: Mutex<Vec<Event>>,
    changed: Notify,
}

/// One device: a client and what it needs to restart.
pub struct Device {
    /// The client.
    pub client: Client,
    /// Its keys (shared by a restarted client).
    pub keys: MemoryKeyStore,
    /// Its clock.
    pub clock: ManualClock,
    dir: tempfile::TempDir,
    log: Arc<Log>,
    seen: usize,
}

async fn open(
    dir: &std::path::Path,
    keys: &MemoryKeyStore,
    clock: &ManualClock,
) -> (Client, Arc<Log>) {
    let mut client = Client::open(
        ClientConfig::new(dir, Arc::new(keys.clone())).with_clock(Arc::new(clock.clone())),
    )
    .await
    .check("open client");
    let mut events = client.take_events().check("event stream");
    let log = Arc::new(Log::default());
    let sink = Arc::clone(&log);
    tokio::spawn(async move {
        while let Some(event) = events.next().await {
            sink.events.lock().push(event);
            sink.changed.notify_waiters();
        }
    });
    (client, log)
}

impl Device {
    /// A fresh device on its own clock.
    pub async fn new() -> Self {
        Self::with_clock(manual_clock()).await
    }

    /// A fresh device on `clock`.
    pub async fn with_clock(clock: ManualClock) -> Self {
        let dir = super::temp_dir();
        let keys = MemoryKeyStore::new();
        let (client, log) = open(dir.path(), &keys, &clock).await;
        Self {
            client,
            keys,
            clock,
            dir,
            log,
            seen: 0,
        }
    }

    /// Simulates a crash and restart: drops the client (its engine stops at
    /// once) and opens a new one on the same database and keys, then starts
    /// it.
    pub async fn restart(&mut self) {
        let (client, log) = open(self.dir.path(), &self.keys, &self.clock).await;
        // Dropping the old client aborts its engine.
        self.client = client;
        self.log = log;
        self.seen = 0;
        self.client.start().await.check("start after restart");
    }

    /// The data directory.
    pub fn data_dir(&self) -> &std::path::Path {
        self.dir.path()
    }

    /// Every event so far.
    pub fn events(&self) -> Vec<Event> {
        self.log.events.lock().clone()
    }

    /// Waits for the next event (after those already matched) for which `f`
    /// holds, and returns it.
    pub async fn wait_event(&mut self, what: &str, f: impl Fn(&Event) -> bool) -> Event {
        let deadline = tokio::time::Instant::now() + WAIT;
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

    /// Whether an event for which `f` holds arrives within [`QUIET`] (after
    /// those already matched).
    pub async fn sees_within_quiet(&mut self, f: impl Fn(&Event) -> bool) -> bool {
        tokio::time::sleep(QUIET).await;
        let events = self.log.events.lock();
        events[self.seen..].iter().any(f)
    }

    /// Waits until `f` holds for this device's client.
    pub async fn wait_until(&self, what: &str, f: impl Fn(&Client) -> bool) {
        let deadline = tokio::time::Instant::now() + WAIT;
        while !f(&self.client) {
            if tokio::time::Instant::now() >= deadline {
                panic!("timed out waiting for {what}; events: {:?}", self.events());
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    }

    /// Waits until connected.
    pub async fn wait_connected(&mut self) {
        self.wait_event("connected", |e| {
            matches!(e, Event::Connection(ConnectionState::Connected))
        })
        .await;
    }

    /// This device's id.
    pub fn device_id(&self) -> [u8; 16] {
        self.client
            .device_id()
            .check("device id")
            .check("registered device")
    }

    /// This device's user.
    pub fn user(&self) -> UserId {
        self.client
            .user_id()
            .check("user id")
            .check("registered user")
    }

    /// The group's members as this device sees them.
    pub fn members(&self) -> Vec<Member> {
        self.client.members().check("members")
    }

    /// The group's epoch on this device (`None`: not in the group).
    pub fn epoch(&self) -> Option<u64> {
        self.client.epoch().check("epoch")
    }

    /// Waits until this device is at `epoch`.
    pub async fn wait_epoch(&self, epoch: u64) {
        self.wait_until(&format!("epoch {epoch}"), |c| {
            c.epoch().ok().flatten() == Some(epoch)
        })
        .await;
    }

    /// Waits until this device sees exactly `members` (any order).
    pub async fn wait_members(&self, members: &[Member]) {
        let mut wanted = members.to_vec();
        wanted.sort();
        self.wait_until("the expected members", |c| {
            c.members().ok().as_ref() == Some(&wanted)
        })
        .await;
    }
}

/// This device as a [`Member`].
pub fn member(device: &Device) -> Member {
    Member {
        user: device.user(),
        device: device.device_id(),
    }
}

/// The admin's first device, registered from the bootstrap invite and
/// started.
pub async fn admin(pi: &TestPi) -> Device {
    let mut admin = Device::new().await;
    let id = admin
        .client
        .register(&pi.bootstrap)
        .await
        .check("register admin");
    assert_eq!(id, user(ADMIN));
    admin.client.start().await.check("start admin");
    admin.wait_connected().await;
    admin
}

/// The admin with the group created.
pub async fn admin_with_group(pi: &TestPi) -> Device {
    let admin = admin(pi).await;
    admin.client.create_group().await.check("create group");
    admin
}

/// A new user invited by the admin, registered and started (not added).
pub async fn invited(admin: &Device, username: &str) -> Device {
    let invite = admin
        .client
        .create_invite(username)
        .await
        .check("create invite");
    let mut device = Device::new().await;
    let id = device
        .client
        .register(&invite.payload)
        .await
        .check("register");
    assert_eq!(id, user(username));
    device.client.start().await.check("start");
    device.wait_connected().await;
    device
}

/// A new user invited, registered, started and added to the group by the
/// admin; waits until the device joined.
pub async fn member_of_group(admin: &Device, username: &str) -> Device {
    let mut device = invited(admin, username).await;
    admin
        .client
        .add_user(&user(username))
        .await
        .check("add user");
    device
        .wait_event("joined the group", |e| matches!(e, Event::JoinedGroup))
        .await;
    device
}
