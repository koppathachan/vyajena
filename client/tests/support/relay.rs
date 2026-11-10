//! A dumb TCP relay in front of the test Pi, as the GCP relay is in front of
//! the real one. Tests cut it to simulate a network drop: open connections
//! break and new ones are refused until it is restored.

use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use parking_lot::Mutex;
use tokio::net::{TcpListener, TcpStream};
use tokio::task::AbortHandle;

use super::Check;

/// The relay.
pub struct Relay {
    /// Where clients connect (the invites' relay address).
    pub addr: SocketAddr,
    up: Arc<AtomicBool>,
    open: Arc<Mutex<Vec<AbortHandle>>>,
    accept: AbortHandle,
}

impl Relay {
    /// A relay on a free local port forwarding to `target`.
    pub async fn start(target: SocketAddr) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.check("bind relay");
        let addr = listener.local_addr().check("relay addr");
        let up = Arc::new(AtomicBool::new(true));
        let open: Arc<Mutex<Vec<AbortHandle>>> = Arc::default();
        let (up2, open2) = (Arc::clone(&up), Arc::clone(&open));
        let accept = tokio::spawn(async move {
            while let Ok((mut inbound, _)) = listener.accept().await {
                if !up2.load(Ordering::SeqCst) {
                    drop(inbound);
                    continue;
                }
                let task = tokio::spawn(async move {
                    if let Ok(mut outbound) = TcpStream::connect(target).await {
                        let _ = tokio::io::copy_bidirectional(&mut inbound, &mut outbound).await;
                    }
                });
                open2.lock().push(task.abort_handle());
            }
        })
        .abort_handle();
        Self {
            addr,
            up,
            open,
            accept,
        }
    }

    /// Breaks every open connection and refuses new ones.
    pub fn cut(&self) {
        self.up.store(false, Ordering::SeqCst);
        for task in self.open.lock().drain(..) {
            task.abort();
        }
    }

    /// Accepts connections again.
    pub fn restore(&self) {
        self.up.store(true, Ordering::SeqCst);
    }
}

impl Drop for Relay {
    fn drop(&mut self) {
        self.accept.abort();
        for task in self.open.lock().drain(..) {
            task.abort();
        }
    }
}
