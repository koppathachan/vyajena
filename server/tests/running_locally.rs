//! pi-runtime: Running locally; pi-runtime: First start (the logged pin).

mod support;

use std::io::{BufRead, BufReader};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::Arc;
use std::sync::mpsc;
use std::time::Duration;

use server::runtime::{Bootstrap, Config, Pi};
use tokio::sync::oneshot;

fn temp() -> tempfile::TempDir {
    match tempfile::tempdir() {
        Ok(dir) => dir,
        Err(error) => panic!("no temp dir: {error}"),
    }
}

// spec: pi-runtime/running-locally
#[test]
fn example_config_listens_on_127_0_0_1_8443() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("local.toml");
    let config = match Config::load(&path) {
        Ok(config) => config,
        Err(error) => panic!("example config refused: {error}"),
    };
    assert_eq!(config.listen_address.to_string(), "127.0.0.1:8443");
    // Unprivileged port, data next to the example, inside the git-ignored folder.
    assert!(config.listen_address.port() >= 1024);
    let local = Path::new(env!("CARGO_MANIFEST_DIR")).join(".pi-local");
    assert!(config.database_path.starts_with(&local));
    assert!(config.key_dir.starts_with(&local));
}

// spec: pi-runtime/running-locally
#[tokio::test]
async fn accepts_a_pinned_tls_connection() {
    let tmp = temp();
    let pi = match Pi::start(
        support::config_in(tmp.path()),
        Arc::new(support::manual_clock()),
    )
    .await
    {
        Ok(pi) => pi,
        Err(error) => panic!("server did not start: {error}"),
    };
    let addr = pi.local_addr();
    let pin = pi.state().keys.tls_spki_sha256();
    let (stop, stopped) = oneshot::channel::<()>();
    let server = tokio::spawn(pi.serve(async {
        let _ = stopped.await;
    }));

    let tls = support::tls_client::connect_pinned(addr, pin).await;
    let tls = match tls {
        Ok(tls) => tls,
        Err(error) => panic!("pinned TLS connection failed: {error}"),
    };
    let (_, session) = tls.get_ref();
    assert_eq!(
        session.protocol_version(),
        Some(rustls::ProtocolVersion::TLSv1_3)
    );
    assert_eq!(session.alpn_protocol(), Some(&b"h2"[..]));
    drop(tls);

    // Any other pin is refused before a byte of gRPC is sent.
    let mut wrong = pin;
    wrong[0] ^= 1;
    assert!(
        support::tls_client::connect_pinned(addr, wrong)
            .await
            .is_err()
    );

    let _ = stop.send(());
    let served = tokio::time::timeout(Duration::from_secs(5), server).await;
    assert!(matches!(served, Ok(Ok(Ok(())))), "{served:?}");
}

/// Runs the server binary on `config_path` and returns its log lines up to and
/// including the "listening" line.
fn run_until_listening(config_path: &Path) -> Vec<String> {
    let child = Command::new(env!("CARGO_BIN_EXE_server"))
        .arg("--config")
        .arg(config_path)
        .env("RUST_LOG", "info")
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn();
    let mut child: Child = match child {
        Ok(child) => child,
        Err(error) => panic!("cannot run the server binary: {error}"),
    };
    let Some(stdout) = child.stdout.take() else {
        panic!("no stdout");
    };
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            let done = line.contains("listening");
            if tx.send(line).is_err() || done {
                break;
            }
        }
    });
    let mut lines = Vec::new();
    while let Ok(line) = rx.recv_timeout(Duration::from_secs(20)) {
        let done = line.contains("listening");
        lines.push(line);
        if done {
            break;
        }
    }
    let _ = child.kill();
    let _ = child.wait();
    lines
}

fn logged(lines: &[String], key: &str) -> Option<String> {
    lines.iter().find_map(|line| {
        line.split_whitespace()
            .find_map(|part| part.strip_prefix(&format!("{key}=")))
            .map(str::to_owned)
    })
}

// spec: pi-runtime/running-locally
// spec: pi-runtime/first-start
#[test]
fn binary_prints_the_pin_and_bootstrap_invite_then_listens() {
    let tmp = temp();
    let config_path = tmp.path().join("pi.toml");
    std::fs::write(&config_path, support::config_text()).ok();

    let first = run_until_listening(&config_path);
    assert!(first.iter().any(|l| l.contains("listening")), "{first:#?}");
    let pin = logged(&first, "tls_spki_sha256");
    assert_eq!(pin.as_ref().map(String::len), Some(64), "{first:#?}");
    assert!(logged(&first, "payload").is_some(), "{first:#?}");
    assert!(logged(&first, "invite_code").is_some(), "{first:#?}");
    assert_eq!(logged(&first, "user_id").as_deref(), Some("akhil@pi"));

    // Restart keeps the pin, and the invite is printed only once.
    let second = run_until_listening(&config_path);
    assert_eq!(logged(&second, "tls_spki_sha256"), pin, "{second:#?}");
    assert!(logged(&second, "payload").is_none(), "{second:#?}");
}

// spec: device-identity/bootstrap-admin
#[tokio::test]
async fn in_process_start_reports_the_invite() {
    let tmp = temp();
    let pi = Pi::start(
        support::config_in(tmp.path()),
        Arc::new(support::manual_clock()),
    )
    .await;
    assert!(matches!(
        pi.map(|p| p.bootstrap().clone()),
        Ok(Bootstrap::Created(_))
    ));
}
