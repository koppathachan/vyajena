//! A dumb TCP relay in front of the test server, as the GCP relay is in front of
//! the real one. Tests cut it to simulate a network drop (or the server's tunnel
//! being down): it still accepts connections but closes them at once, and open
//! ones break, until it is restored. It can record every byte it forwards
//! (what the real relay sees) and be pointed at another server.

use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};

use parking_lot::Mutex;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};
use tokio::net::{TcpListener, TcpStream};
use tokio::task::AbortHandle;

use super::Check;

#[derive(Default)]
struct Shared {
    up: AtomicBool,
    accepted: AtomicUsize,
    /// Bytes forwarded from devices to the server, and back.
    bytes_up: AtomicU64,
    bytes_down: AtomicU64,
    recording: AtomicBool,
    recorded: Mutex<Vec<u8>>,
    open: Mutex<Vec<AbortHandle>>,
}

/// The relay.
pub struct Relay {
    /// Where clients connect (the invites' relay address).
    pub addr: SocketAddr,
    target: Arc<Mutex<SocketAddr>>,
    shared: Arc<Shared>,
    accept: AbortHandle,
}

/// Copies one direction (`upstream`: device to server), counting what passes and
/// recording it while recording is on.
async fn pipe(
    mut from: OwnedReadHalf,
    mut to: OwnedWriteHalf,
    shared: Arc<Shared>,
    upstream: bool,
) {
    let mut buf = vec![0u8; 16 * 1024];
    loop {
        let n = match from.read(&mut buf).await {
            Ok(0) | Err(_) => break,
            Ok(n) => n,
        };
        let Some(chunk) = buf.get(..n) else { break };
        let count = if upstream {
            &shared.bytes_up
        } else {
            &shared.bytes_down
        };
        count.fetch_add(n as u64, Ordering::SeqCst);
        if shared.recording.load(Ordering::SeqCst) {
            shared.recorded.lock().extend_from_slice(chunk);
        }
        if to.write_all(chunk).await.is_err() {
            break;
        }
    }
    let _ = to.shutdown().await;
}

impl Relay {
    /// A relay on a free local port forwarding to `target`.
    pub async fn start(target: SocketAddr) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.check("bind relay");
        let addr = listener.local_addr().check("relay addr");
        let shared = Arc::new(Shared::default());
        shared.up.store(true, Ordering::SeqCst);
        let target = Arc::new(Mutex::new(target));
        let (shared2, target2) = (Arc::clone(&shared), Arc::clone(&target));
        let accept = tokio::spawn(async move {
            while let Ok((inbound, _)) = listener.accept().await {
                shared2.accepted.fetch_add(1, Ordering::SeqCst);
                if !shared2.up.load(Ordering::SeqCst) {
                    drop(inbound);
                    continue;
                }
                let to = *target2.lock();
                let shared3 = Arc::clone(&shared2);
                let task = tokio::spawn(async move {
                    if let Ok(outbound) = TcpStream::connect(to).await {
                        let (in_read, in_write) = inbound.into_split();
                        let (out_read, out_write) = outbound.into_split();
                        let up = tokio::spawn(pipe(in_read, out_write, Arc::clone(&shared3), true));
                        // A cut aborts both directions.
                        shared3.open.lock().push(up.abort_handle());
                        pipe(out_read, in_write, shared3, false).await;
                        let _ = up.await;
                    }
                });
                shared2.open.lock().push(task.abort_handle());
            }
        })
        .abort_handle();
        Self {
            addr,
            target,
            shared,
            accept,
        }
    }

    /// Breaks every open connection; new ones are accepted and closed at once.
    pub fn cut(&self) {
        self.shared.up.store(false, Ordering::SeqCst);
        for task in self.shared.open.lock().drain(..) {
            task.abort();
        }
    }

    /// Forwards connections again.
    pub fn restore(&self) {
        self.shared.up.store(true, Ordering::SeqCst);
    }

    /// Forwards new connections to `target` from now on.
    pub fn retarget(&self, target: SocketAddr) {
        *self.target.lock() = target;
    }

    /// TCP connections accepted so far, forwarded or not.
    pub fn accepted(&self) -> usize {
        self.shared.accepted.load(Ordering::SeqCst)
    }

    /// Bytes forwarded so far from devices to the server.
    pub fn bytes_up(&self) -> u64 {
        self.shared.bytes_up.load(Ordering::SeqCst)
    }

    /// Bytes forwarded so far from the server to devices.
    pub fn bytes_down(&self) -> u64 {
        self.shared.bytes_down.load(Ordering::SeqCst)
    }

    /// Starts recording every byte forwarded, both ways.
    pub fn record(&self) {
        self.shared.recording.store(true, Ordering::SeqCst);
    }

    /// The bytes recorded so far.
    pub fn recorded(&self) -> Vec<u8> {
        self.shared.recorded.lock().clone()
    }
}

impl Drop for Relay {
    fn drop(&mut self) {
        self.accept.abort();
        for task in self.shared.open.lock().drain(..) {
            task.abort();
        }
    }
}
