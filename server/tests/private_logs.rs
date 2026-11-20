//! server-runtime (relay-privacy-logging): the server's logs carry a device id only as
//! a short stable hash, never raw, and never a session token; a stream open
//! emits the connection metrics. This binary installs a global log capture, so
//! it holds one test.

mod support;

use std::io::Write;
use std::sync::{Arc, Mutex, PoisonError};

use server::runtime::redact;
use server::runtime::server_keys::hex;
use support::messaging::world;

#[derive(Clone, Default)]
struct Captured(Arc<Mutex<Vec<u8>>>);

impl Write for Captured {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl Captured {
    fn text(&self) -> String {
        let bytes = self
            .0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone();
        String::from_utf8_lossy(&bytes).into_owned()
    }
}

// spec: server-runtime/private-logs
// spec: server-runtime/connection-metrics
#[tokio::test]
async fn logs_redact_device_ids_and_emit_stream_metrics() {
    let captured = Captured::default();
    let writer = captured.clone();
    let subscriber = tracing_subscriber::fmt()
        .with_ansi(false)
        .with_max_level(tracing::Level::DEBUG)
        .with_writer(move || writer.clone())
        .finish();
    tracing::subscriber::set_global_default(subscriber).expect("log capture");

    // Bootstraps the admin and two more devices, each opening a Connect stream.
    let w = world().await;
    let logs = captured.text();

    // A stream open emits the redacted metrics event.
    let opens: Vec<&str> = logs.lines().filter(|l| l.contains("stream open")).collect();
    assert!(!opens.is_empty(), "no stream-open metrics event:\n{logs}");
    assert!(
        opens.iter().all(|l| l.contains("open_ms")
            && l.contains("reopens")
            && l.contains("active_streams")),
        "metrics fields missing: {opens:?}"
    );

    // The phone's device id appears only as its redacted hash, never raw hex,
    // and its session token never appears.
    let raw_hex = hex(&w.phone.device_id);
    let hash = redact(&w.phone.device_id).to_string();
    assert!(logs.contains(&hash), "redacted device hash should appear");
    assert!(
        !logs.contains(&raw_hex),
        "raw device id hex must never be logged: {raw_hex}"
    );
    assert!(
        !logs.contains(w.phone.token()),
        "a session token must never be logged"
    );
}
