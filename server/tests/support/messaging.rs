//! `Messaging.Connect` from the test side: a stream wrapper that sends frames
//! and reads them with a timeout, envelope builders, and a group of real
//! OpenMLS members built entirely over the stream.

use std::collections::VecDeque;
use std::time::Duration;

use protocol::wire_transport::PROTOCOL_VERSION;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;

use super::clients::{TestDevice, TestUser, random32};
use super::harness::{Conn, TestPi, authed};
use super::pb;
use super::pb::client_frame::Frame as In;
use super::pb::envelope::Kind;
use super::pb::server_frame::Frame as Out;
use super::{Check, bootstrap_admin, invite_user, pi_external_sender};

/// How long a test waits for a frame it expects.
pub const WAIT: Duration = Duration::from_secs(10);

/// How long a test waits to be sure nothing arrives.
pub const QUIET: Duration = Duration::from_millis(300);

/// The group id the tests use.
pub const GROUP_ID: &[u8] = b"the-one-group";

/// A fresh 16-byte `msg_id`.
pub fn msg_id() -> Vec<u8> {
    random32()[..16].to_vec()
}

/// An envelope as a client builds it.
pub fn envelope(kind: Kind, epoch: u64, ciphertext: Vec<u8>) -> pb::Envelope {
    pb::Envelope {
        msg_id: msg_id(),
        group_id: GROUP_ID.to_vec(),
        epoch,
        kind: kind as i32,
        ciphertext,
        ..Default::default()
    }
}

/// A `COMMIT` envelope with the Welcome for `joiners`.
pub fn commit_envelope(
    epoch: u64,
    commit: Vec<u8>,
    welcome: Option<(Vec<u8>, &[&TestDevice])>,
) -> pb::Envelope {
    let mut envelope = envelope(Kind::Commit, epoch, commit);
    envelope.welcome = welcome.map(|(welcome, joiners)| pb::WelcomeBundle {
        welcome,
        recipients: joiners.iter().map(|d| d.device_ref()).collect(),
    });
    envelope
}

/// One open `Connect` stream.
pub struct Stream {
    tx: mpsc::Sender<pb::ClientFrame>,
    rx: tonic::Streaming<pb::ServerFrame>,
    stash: VecDeque<Out>,
    /// `JoinNotice` frames, kept apart so tests about other frames see the
    /// same sequence as before automatic add (read with
    /// [`Stream::join_notice`]).
    joins: VecDeque<pb::JoinNotice>,
}

impl Stream {
    /// Opens a stream with `token` and sends `first` as its first frame.
    pub async fn open_with(
        conn: &Conn,
        token: &str,
        first: Option<pb::ClientFrame>,
    ) -> Result<Self, tonic::Status> {
        let (tx, frames) = mpsc::channel(64);
        if let Some(first) = first {
            tx.send(first).await.check("first frame");
        }
        let rx = conn
            .messaging()
            .connect(authed(token, ReceiverStream::new(frames)))
            .await?
            .into_inner();
        Ok(Self {
            tx,
            rx,
            stash: VecDeque::new(),
            joins: VecDeque::new(),
        })
    }

    /// Opens `device`'s stream and says Hello with the supported version.
    pub async fn open(conn: &Conn, device: &TestDevice) -> Self {
        let hello = pb::ClientFrame {
            frame: Some(In::Hello(pb::Hello {
                protocol_version: PROTOCOL_VERSION,
            })),
        };
        Self::open_with(conn, device.token(), Some(hello))
            .await
            .check("Connect")
    }

    /// The next message from the server within `wait`, with `JoinNotice`
    /// frames set aside.
    async fn recv(
        &mut self,
        wait: Duration,
    ) -> Result<Result<Option<pb::ServerFrame>, tonic::Status>, tokio::time::error::Elapsed> {
        let deadline = tokio::time::Instant::now() + wait;
        loop {
            let message = tokio::time::timeout_at(deadline, self.rx.message()).await?;
            match message {
                Ok(Some(pb::ServerFrame {
                    frame: Some(Out::JoinNotice(notice)),
                })) => self.joins.push_back(notice),
                other => return Ok(other),
            }
        }
    }

    /// The next `JoinNotice` (group-membership: Automatic add of new users);
    /// other frames before it are kept for [`Stream::next`].
    pub async fn join_notice(&mut self) -> pb::JoinNotice {
        let mut skipped = VecDeque::new();
        let found = loop {
            if let Some(notice) = self.joins.pop_front() {
                break notice;
            }
            match tokio::time::timeout(WAIT, self.rx.message()).await {
                Ok(Ok(Some(pb::ServerFrame {
                    frame: Some(Out::JoinNotice(notice)),
                }))) => break notice,
                Ok(Ok(Some(pb::ServerFrame { frame: Some(frame) }))) => skipped.push_back(frame),
                other => panic!("expected a JoinNotice, got {other:?}"),
            }
        };
        self.stash.extend(skipped);
        found
    }

    /// Drops the `JoinNotice` frames received so far.
    pub fn forget_join_notices(&mut self) {
        self.joins.clear();
    }

    /// Asserts no `JoinNotice` arrives for a short while (other frames are
    /// kept for [`Stream::next`]).
    pub async fn assert_no_join_notice(&mut self) {
        assert!(
            self.joins.is_empty(),
            "unexpected notices: {:?}",
            self.joins
        );
        let deadline = tokio::time::Instant::now() + QUIET;
        while let Ok(message) = tokio::time::timeout_at(deadline, self.rx.message()).await {
            match message {
                Ok(Some(pb::ServerFrame {
                    frame: Some(Out::JoinNotice(notice)),
                })) => panic!("expected no JoinNotice, got {notice:?}"),
                Ok(Some(pb::ServerFrame { frame: Some(frame) })) => self.stash.push_back(frame),
                other => panic!("the stream ended: {other:?}"),
            }
        }
    }

    /// Sends a frame.
    pub async fn send(&self, frame: In) {
        self.tx
            .send(pb::ClientFrame { frame: Some(frame) })
            .await
            .check("send frame");
    }

    /// Sends an envelope.
    pub async fn send_envelope(&self, envelope: pb::Envelope) {
        self.send(In::Envelope(envelope)).await;
    }

    /// Sends a cumulative ack.
    pub async fn ack(&self, server_seq: u64) {
        self.send(In::Ack(pb::Ack {
            group_id: GROUP_ID.to_vec(),
            server_seq,
        }))
        .await;
    }

    /// Round-trips a heartbeat, so every frame sent before (acks included)
    /// has been processed. Frames that arrive first are kept for
    /// [`Stream::next`].
    pub async fn sync(&mut self) {
        self.send(In::Heartbeat(pb::Heartbeat {})).await;
        let mut skipped = VecDeque::new();
        loop {
            let frame = match self.recv(WAIT).await {
                Ok(Ok(Some(pb::ServerFrame { frame: Some(frame) }))) => frame,
                other => panic!("expected a heartbeat, got {other:?}"),
            };
            match frame {
                Out::Heartbeat(_) => break,
                other => skipped.push_back(other),
            }
        }
        self.stash.extend(skipped);
    }

    /// The next frame from the server, or the status the stream ended with.
    pub async fn try_next(&mut self) -> Result<Option<Out>, tonic::Status> {
        if let Some(frame) = self.stash.pop_front() {
            return Ok(Some(frame));
        }
        match self.recv(WAIT).await {
            Ok(Ok(Some(frame))) => Ok(frame.frame),
            Ok(Ok(None)) => Ok(None),
            Ok(Err(status)) => Err(status),
            Err(_) => panic!("no frame within {WAIT:?}"),
        }
    }

    /// The next frame; panics if the stream ends.
    pub async fn next(&mut self) -> Out {
        match self.try_next().await {
            Ok(Some(frame)) => frame,
            other => panic!("expected a frame, got {other:?}"),
        }
    }

    /// The next envelope; panics on any other frame.
    pub async fn envelope(&mut self) -> pb::Envelope {
        match self.next().await {
            Out::Envelope(envelope) => envelope,
            other => panic!("expected an envelope, got {other:?}"),
        }
    }

    /// The next `SendResult`; other frames before it are kept for
    /// [`Stream::next`].
    pub async fn result(&mut self) -> pb::SendResult {
        let mut skipped = VecDeque::new();
        let found = loop {
            let frame = match self.stash.pop_front() {
                Some(frame) => frame,
                None => match self.recv(WAIT).await {
                    Ok(Ok(Some(pb::ServerFrame { frame: Some(frame) }))) => frame,
                    other => panic!("expected a SendResult, got {other:?}"),
                },
            };
            match frame {
                Out::SendResult(result) => break result,
                other => skipped.push_back(other),
            }
        };
        skipped.extend(self.stash.drain(..));
        self.stash = skipped;
        found
    }

    /// Sends `envelope` and expects `Accepted`; returns its `server_seq`.
    pub async fn accepted(&mut self, envelope: pb::Envelope) -> u64 {
        let msg_id = envelope.msg_id.clone();
        self.send_envelope(envelope).await;
        let result = self.result().await;
        assert_eq!(result.msg_id, msg_id);
        match result.result {
            Some(pb::send_result::Result::Accepted(accepted)) => accepted.server_seq,
            other => panic!("expected Accepted, got {other:?}"),
        }
    }

    /// Sends `envelope` and expects a rejection; returns it.
    pub async fn rejected(&mut self, envelope: pb::Envelope) -> pb::send_result::Rejected {
        let msg_id = envelope.msg_id.clone();
        self.send_envelope(envelope).await;
        let result = self.result().await;
        assert_eq!(result.msg_id, msg_id);
        match result.result {
            Some(pb::send_result::Result::Rejected(rejected)) => rejected,
            other => panic!("expected Rejected, got {other:?}"),
        }
    }

    /// Asserts no frame arrives for a short while.
    pub async fn assert_quiet(&mut self) {
        assert!(self.stash.is_empty(), "unexpected frames: {:?}", self.stash);
        if let Ok(frame) = self.recv(QUIET).await {
            panic!("expected no frame, got {frame:?}");
        }
    }

    /// Waits for the stream to end and returns its status (`Ok` for a clean
    /// end). Frames before the end are discarded.
    pub async fn ended(&mut self) -> Result<(), tonic::Status> {
        loop {
            match self.recv(WAIT).await {
                Ok(Ok(Some(_))) => continue,
                Ok(Ok(None)) => return Ok(()),
                Ok(Err(status)) => return Err(status),
                Err(_) => panic!("stream did not end within {WAIT:?}"),
            }
        }
    }
}

/// Reads the next envelope on `stream`, applies it to `device` (commit,
/// Welcome or proposal) and acks it. Returns it.
pub async fn receive(stream: &mut Stream, device: &mut TestDevice) -> pb::Envelope {
    let envelope = stream.envelope().await;
    match Kind::try_from(envelope.kind) {
        Ok(Kind::Commit) => device.receive_commit(&envelope.ciphertext),
        Ok(Kind::Welcome) => device.join(&envelope.ciphertext),
        Ok(Kind::Proposal) => device.receive_proposal(&envelope.ciphertext),
        Ok(Kind::Application) => {
            device.read_message(&envelope.ciphertext);
        }
        other => panic!("unexpected kind {other:?}"),
    }
    stream.ack(envelope.server_seq).await;
    envelope
}

/// The admin with the group created, and Alice's laptop and Bob's phone
/// added over `Connect`, each with an open stream.
pub struct World {
    pub pi: TestPi,
    pub conn: Conn,
    pub admin_user: TestUser,
    pub admin: TestDevice,
    pub admin_stream: Stream,
    pub alice: TestUser,
    pub laptop: TestDevice,
    pub laptop_stream: Stream,
    pub bob: TestUser,
    pub phone: TestDevice,
    pub phone_stream: Stream,
}

/// Fetches one KeyPackage of each of `user`'s devices.
pub async fn fetch(conn: &Conn, caller: &TestDevice, user: &TestUser) -> Vec<Vec<u8>> {
    conn.keys()
        .fetch_key_packages(authed(
            caller.token(),
            pb::FetchKeyPackagesRequest {
                user_id: Some(user.pb_user_id()),
                device_ids: vec![],
                group_id: Vec::new(),
            },
        ))
        .await
        .check("FetchKeyPackages")
        .into_inner()
        .devices
        .into_iter()
        .map(|d| d.key_package)
        .collect()
}

/// `committer` adds `key_packages` over its stream; each joiner reads its
/// Welcome from its own stream and acks; each member processes the commit.
pub async fn add_over_stream(
    committer: &mut TestDevice,
    committer_stream: &mut Stream,
    key_packages: &[Vec<u8>],
    members: &mut [(&mut TestDevice, &mut Stream)],
    joiners: &mut [(&mut TestDevice, &mut Stream)],
) -> u64 {
    let epoch = committer.epoch();
    let (commit, welcome) = committer.add(key_packages);
    let refs: Vec<&TestDevice> = joiners.iter().map(|(d, _)| &**d).collect();
    let seq = committer_stream
        .accepted(commit_envelope(epoch, commit, Some((welcome, &refs))))
        .await;
    committer.confirm();
    for (member, stream) in members.iter_mut() {
        let got = receive(stream, member).await;
        assert_eq!(got.server_seq, seq);
    }
    for (joiner, stream) in joiners.iter_mut() {
        let got = receive(stream, joiner).await;
        assert_eq!(got.kind, Kind::Welcome as i32);
        assert_eq!(got.server_seq, seq);
    }
    seq
}

/// `device` sends `text` in its current epoch and expects `Accepted`;
/// returns the `server_seq`.
pub async fn say(device: &mut TestDevice, stream: &mut Stream, text: &[u8]) -> u64 {
    let epoch = device.epoch();
    let ciphertext = device.message(text);
    stream
        .accepted(envelope(Kind::Application, epoch, ciphertext))
        .await
}

/// Authenticates `device` again (tokens last an hour) and opens a stream
/// whose connect-time work is done.
pub async fn reconnect(conn: &Conn, device: &mut TestDevice) -> Stream {
    device.authenticate(conn).await;
    let mut stream = Stream::open(conn, device).await;
    stream.sync().await;
    stream
}

/// A stream that never says Hello, so `device` counts as offline; use it to
/// replace (and so close) a device's real stream.
pub async fn offline(conn: &Conn, device: &TestDevice) -> Stream {
    Stream::open_with(conn, device.token(), None)
        .await
        .check("Connect without Hello")
}

/// Waits until the Pi no longer has a stream for `device`.
pub async fn wait_offline(pi: &TestPi, device: &TestDevice) {
    let deadline = tokio::time::Instant::now() + WAIT;
    while pi.state.hub.is_connected(&device.device_id) {
        assert!(
            tokio::time::Instant::now() < deadline,
            "the device's stream did not close"
        );
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
}

/// Builds the [`World`] on a fresh test server.
pub async fn world() -> World {
    world_on(TestPi::start().await).await
}

/// Builds the [`World`] on `pi`.
pub async fn world_on(pi: TestPi) -> World {
    let conn = pi.connect().await;
    let (admin_user, mut admin) = bootstrap_admin(&pi, &conn).await;
    let pi_sender = pi_external_sender(&conn).await;
    let request = admin.create_group(GROUP_ID, &pi_sender, "akhil@pi");
    conn.admin()
        .create_group(authed(admin.token(), request))
        .await
        .check("CreateGroup");
    let (alice, mut laptop) = invite_user(&conn, &admin, "alice").await;
    let (bob, mut phone) = invite_user(&conn, &admin, "bob").await;
    // Enough one-time KeyPackages that nobody gets a low-KeyPackages notice.
    admin.upload_key_packages(&conn, 10).await;
    laptop.upload_key_packages(&conn, 10).await;
    phone.upload_key_packages(&conn, 10).await;

    let mut admin_stream = Stream::open(&conn, &admin).await;
    let mut laptop_stream = Stream::open(&conn, &laptop).await;
    let mut phone_stream = Stream::open(&conn, &phone).await;
    let mut key_packages = fetch(&conn, &admin, &alice).await;
    key_packages.extend(fetch(&conn, &admin, &bob).await);
    add_over_stream(
        &mut admin,
        &mut admin_stream,
        &key_packages,
        &mut [],
        &mut [
            (&mut laptop, &mut laptop_stream),
            (&mut phone, &mut phone_stream),
        ],
    )
    .await;
    laptop_stream.sync().await;
    phone_stream.sync().await;
    // The admin's stream heard about the laptop and the phone before they
    // were added (group-membership: Automatic add of new users).
    admin_stream.sync().await;
    admin_stream.forget_join_notices();
    World {
        pi,
        conn,
        admin_user,
        admin,
        admin_stream,
        alice,
        laptop,
        laptop_stream,
        bob,
        phone,
        phone_stream,
    }
}
