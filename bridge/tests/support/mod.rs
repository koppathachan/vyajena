//! Shared helpers for the bridge tests: the real server in-process, with the
//! client tests' own helpers (`client/tests/support/{server,relay}.rs`, included
//! by path so there is one copy), and a [`device::Device`] around the
//! bridge's `Core`.
#![allow(dead_code)]

pub mod device;
#[path = "../../../client/tests/support/relay.rs"]
pub mod relay;
#[path = "../../../client/tests/support/server.rs"]
pub mod server;

use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Logs to the test output when `RUST_LOG` is set (e.g. `client=debug`).
pub fn init_logging() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        if std::env::var_os("RUST_LOG").is_some() {
            let _ = tracing_subscriber::fmt()
                .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
                .with_test_writer()
                .try_init();
        }
    });
}

/// Turns a failure into a test panic naming what failed.
pub trait Check<T> {
    /// The value, or a panic with `what` and the error.
    fn check(self, what: &str) -> T;
}

impl<T, E: std::fmt::Debug> Check<T> for Result<T, E> {
    fn check(self, what: &str) -> T {
        match self {
            Ok(value) => value,
            Err(error) => panic!("{what} failed: {error:?}"),
        }
    }
}

impl<T> Check<T> for Option<T> {
    fn check(self, what: &str) -> T {
        match self {
            Some(value) => value,
            None => panic!("{what}: nothing there"),
        }
    }
}

/// 2026-11-10T00:00:00Z, the start of every manual clock (as in the client
/// tests, so devices and the test server agree on the time).
pub fn start_time() -> SystemTime {
    UNIX_EPOCH + Duration::from_secs(1_794_268_800)
}
