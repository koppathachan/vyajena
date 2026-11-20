//! The real server binary (`CARGO_BIN_EXE_server`) in a temp folder: started,
//! stopped with a signal, restarted on the same database and keys.

use std::io::{BufRead, BufReader};
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use super::clients::{TestDevice, TestUser};
use super::harness::{Conn, authed};
use super::messaging::{GROUP_ID, Stream};
use super::{Check, config_text, register, server_external_sender};

/// How long the binary gets to start listening or to exit.
pub const WAIT: Duration = Duration::from_secs(20);

/// A server binary process.
pub struct Binary {
    child: Child,
    /// Where it listens (a random local port).
    pub addr: SocketAddr,
    /// Its TLS pin, from the log.
    pub pin: [u8; 32],
    /// The bootstrap invite code, logged on first start only.
    pub invite_code: Option<String>,
    /// The log lines up to "listening".
    pub log: Vec<String>,
}

/// A temp folder with a config for the binary (admin `akhil@chat`).
pub struct Home {
    dir: tempfile::TempDir,
}

impl Home {
    /// A fresh folder with `server.toml` in it.
    pub fn new() -> Self {
        let dir = tempfile::tempdir().check("temp dir");
        std::fs::write(dir.path().join("server.toml"), config_text()).check("write server.toml");
        Self { dir }
    }

    /// The folder.
    pub fn path(&self) -> &Path {
        self.dir.path()
    }

    /// The config file.
    pub fn config(&self) -> PathBuf {
        self.dir.path().join("server.toml")
    }

    /// Starts `server --config server.toml` and waits until it listens.
    pub fn start(&self) -> Binary {
        let mut child = Command::new(env!("CARGO_BIN_EXE_server"))
            .arg("--config")
            .arg(self.config())
            .env("RUST_LOG", "info")
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .check("run the server binary");
        let stdout = child.stdout.take().check("stdout");
        let (tx, rx) = mpsc::channel();
        // Keeps reading after "listening" so the server never blocks on a
        // full pipe.
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                let _ = tx.send(line);
            }
        });
        let mut lines = Vec::new();
        let deadline = Instant::now() + WAIT;
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            match rx.recv_timeout(left) {
                Ok(line) => {
                    let done = line.contains("listening");
                    lines.push(line);
                    if done {
                        break;
                    }
                }
                Err(_) => {
                    let _ = child.kill();
                    panic!("the server did not start listening: {lines:#?}");
                }
            }
        }
        let addr = logged(&lines, "listen_address")
            .check("listen_address in the log")
            .parse()
            .check("listen_address");
        let pin = pin_from_hex(&logged(&lines, "tls_spki_sha256").check("pin in the log"));
        let invite_code = logged(&lines, "invite_code");
        Binary {
            child,
            addr,
            pin,
            invite_code,
            log: lines,
        }
    }
}

impl Default for Home {
    fn default() -> Self {
        Self::new()
    }
}

impl Binary {
    /// The process id.
    pub fn pid(&self) -> u32 {
        self.child.id()
    }

    /// Sends `signal` (`TERM`, `INT`, `KILL`) with `kill(1)`.
    pub fn signal(&self, signal: &str) {
        let status = Command::new("kill")
            .arg(format!("-{signal}"))
            .arg(self.pid().to_string())
            .status()
            .check("kill");
        assert!(status.success(), "kill -{signal} failed");
    }

    /// Waits for the process to exit; panics after [`WAIT`]. It yields
    /// while it waits: the test's clients run on the same runtime, and the
    /// server's graceful stop waits for each HTTP/2 client to answer its
    /// shutdown PING.
    pub async fn wait(mut self) -> ExitStatus {
        let deadline = Instant::now() + WAIT;
        loop {
            if let Some(status) = self.child.try_wait().check("try_wait") {
                return status;
            }
            if Instant::now() > deadline {
                let _ = self.child.kill();
                panic!("the server did not exit within {WAIT:?}");
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }
}

impl Drop for Binary {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// The admin registered from the binary's bootstrap invite, with the group
/// created and a stream open.
pub async fn admin_on(server: &Binary) -> (Conn, TestUser, TestDevice, Stream) {
    let conn = Conn::open(server.addr, server.pin).await;
    let admin = TestUser::new("akhil");
    let mut device = admin.new_device();
    let code = server
        .invite_code
        .clone()
        .check("bootstrap invite in the log");
    register(&conn, &code, &admin, &device)
        .await
        .check("Register (bootstrap)");
    device.authenticate(&conn).await;
    let server_sender = server_external_sender(&conn).await;
    let request = device.create_group(GROUP_ID, &server_sender, "akhil@chat");
    conn.admin()
        .create_group(authed(device.token(), request))
        .await
        .check("CreateGroup");
    let stream = Stream::open(&conn, &device).await;
    (conn, admin, device, stream)
}

/// The value of `key=value` in the first log line that has it.
pub fn logged(lines: &[String], key: &str) -> Option<String> {
    let prefix = format!("{key}=");
    lines.iter().find_map(|line| {
        line.split_whitespace()
            .find_map(|part| part.strip_prefix(&prefix))
            .map(str::to_owned)
    })
}

fn pin_from_hex(hex: &str) -> [u8; 32] {
    let mut pin = [0u8; 32];
    assert_eq!(hex.len(), 64, "pin {hex:?}");
    for (i, byte) in pin.iter_mut().enumerate() {
        let pair = hex.get(2 * i..2 * i + 2).check("pin hex");
        *byte = u8::from_str_radix(pair, 16).check("pin hex");
    }
    pin
}
