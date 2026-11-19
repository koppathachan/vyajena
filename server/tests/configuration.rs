//! pi-runtime: Configuration.

mod support;

use std::net::{SocketAddr, TcpStream, UdpSocket};
use std::path::Path;
use std::process::Command;
use std::time::Duration;

use server::runtime::config::BlobStoreConfig;
use server::runtime::{Config, ConfigError};
use support::Check;
use support::binary::{Home, admin_on, logged};

fn parse(text: &str) -> Result<Config, ConfigError> {
    Config::from_toml(text, Path::new("/srv/pi"))
}

/// The test config with one line removed or replaced.
fn without(field: &str) -> String {
    support::config_text()
        .lines()
        .filter(|line| !line.starts_with(&format!("{field} ")))
        .collect::<Vec<_>>()
        .join("\n")
}

fn with(field: &str, value: &str) -> String {
    format!("{}\n{field} = {value}\n", without(field))
}

// spec: pi-runtime/configuration
#[test]
fn reads_every_setting() {
    let config = match parse(&support::config_text()) {
        Ok(config) => config,
        Err(error) => panic!("valid config refused: {error}"),
    };
    assert_eq!(config.server_id, "pi");
    assert_eq!(config.admin_username, "akhil");
    assert_eq!(config.admin_user_id(), "akhil@pi");
    assert_eq!(config.listen_address.to_string(), "127.0.0.1:0");
    assert_eq!(config.public_address, "relay.example:443");
    // Relative paths are resolved from the config file's folder.
    assert_eq!(config.database_path, Path::new("/srv/pi/pi.sqlite"));
    assert_eq!(config.key_dir, Path::new("/srv/pi/keys"));
}

// spec: pi-runtime/configuration
#[test]
fn missing_admin_username_names_the_field() {
    let error = parse(&without("admin_username")).err();
    assert!(
        matches!(
            error,
            Some(ConfigError::Missing {
                field: "admin_username"
            })
        ),
        "{error:?}"
    );
    let message = error.map(|e| e.to_string()).unwrap_or_default();
    assert!(message.contains("admin_username"), "{message}");
}

// spec: pi-runtime/configuration
#[test]
fn every_required_field_is_named_when_missing() {
    for field in [
        "server_id",
        "admin_username",
        "listen_address",
        "database_path",
        "key_dir",
    ] {
        let error = parse(&without(field)).err();
        assert_eq!(error.as_ref().and_then(ConfigError::field), Some(field));
        assert!(
            matches!(error, Some(ConfigError::Missing { .. })),
            "{field}"
        );
    }
}

// spec: pi-runtime/configuration
#[test]
fn public_address_defaults_to_the_listen_address() {
    let text = without("public_address");
    let config = parse(&text).ok();
    assert_eq!(
        config.map(|c| c.public_address),
        Some("127.0.0.1:0".to_owned())
    );
}

// spec: pi-runtime/configuration
#[test]
fn invalid_values_name_their_field() {
    let cases = [
        ("listen_address", r#""localhost""#),
        ("listen_address", r#""127.0.0.1""#),
        ("listen_address", "8443"),
        ("public_address", r#""relay.example""#),
        ("public_address", r#""relay.example:99999""#),
        ("public_address", r#"":443""#),
        ("admin_username", r#""akhil@pi""#),
        ("admin_username", r#""ak hil""#),
        ("admin_username", r#""""#),
        ("server_id", r#""pi@home""#),
        ("server_id", "42"),
        ("database_path", r#""   ""#),
        ("key_dir", "true"),
    ];
    for (field, value) in cases {
        let error = parse(&with(field, value)).err();
        assert!(
            matches!(&error, Some(ConfigError::Invalid { field: f, .. }) if *f == field),
            "{field} = {value}: {error:?}"
        );
    }
}

// spec: pi-runtime/configuration
#[test]
fn unknown_fields_are_refused_by_name() {
    let error = parse(&format!(
        "{}\nadmin_user = \"akhil\"\n",
        support::config_text()
    ))
    .err();
    assert_eq!(
        error.as_ref().and_then(ConfigError::field),
        Some("admin_user")
    );
    assert!(matches!(error, Some(ConfigError::Unknown { .. })));
}

// spec: pi-runtime/configuration
#[test]
fn broken_toml_is_refused() {
    assert!(matches!(
        parse("server_id = \"pi"),
        Err(ConfigError::Syntax(_))
    ));
}

// spec: pi-runtime/configuration
#[test]
fn server_exits_naming_the_field_and_touches_nothing() {
    // Missing admin username: the binary exits with an error naming
    // `admin_username`, before creating keys or a database or opening a port.
    let dir = tempfile::tempdir().ok();
    let dir = dir.as_ref().map(|d| d.path()).unwrap_or(Path::new("."));
    let config_path = dir.join("pi.toml");
    std::fs::write(&config_path, without("admin_username")).ok();

    let output = Command::new(env!("CARGO_BIN_EXE_server"))
        .arg("--config")
        .arg(&config_path)
        .output();
    let output = match output {
        Ok(output) => output,
        Err(error) => panic!("cannot run the server binary: {error}"),
    };
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("admin_username"), "{stderr}");
    assert!(!dir.join("keys").exists());
    assert!(!dir.join("pi.sqlite").exists());
}

/// This machine's address on its outbound interface (no packet is sent), if
/// it has one besides loopback.
fn lan_address() -> Option<std::net::IpAddr> {
    let socket = UdpSocket::bind("0.0.0.0:0").ok()?;
    socket.connect("192.0.2.1:9").ok()?;
    let ip = socket.local_addr().ok()?.ip();
    (!ip.is_loopback() && !ip.is_unspecified()).then_some(ip)
}

// spec: pi-runtime/configuration
#[tokio::test]
async fn behind_the_relay_listens_on_loopback_and_invites_name_the_relay() {
    // The Pi's relay setup (deploy/pi/config.toml): loopback only, the relay's
    // static address in invites.
    let home = Home::new();
    let text = with("public_address", r#""203.0.113.10:443""#);
    std::fs::write(home.config(), text).check("write pi.toml");
    let pi = home.start();

    assert!(pi.addr.ip().is_loopback(), "listens on {}", pi.addr);
    assert_eq!(
        logged(&pi.log, "relay_address").as_deref(),
        Some("203.0.113.10:443"),
        "the bootstrap invite names the relay"
    );
    if let Some(ip) = lan_address() {
        let lan = SocketAddr::new(ip, pi.addr.port());
        assert!(
            TcpStream::connect_timeout(&lan, Duration::from_secs(2)).is_err(),
            "{lan} must not accept: the server is loopback-only"
        );
    }

    let (conn, _admin, device, _stream) = admin_on(&pi).await;
    let response = support::create_invite(&conn, &device, "bob").await;
    let payload = response.payload.check("payload");
    assert_eq!(payload.relay_address, "203.0.113.10:443");
    assert_eq!(payload.tls_spki_sha256, pi.pin.to_vec());
}

fn blob_store(table: &str) -> String {
    format!("{}\n{table}\n", support::config_text())
}

// spec: pi-runtime/configuration
#[test]
fn the_blob_store_is_off_unless_configured() {
    let config = parse(&support::config_text()).check("valid config");
    assert_eq!(config.blob_store, None);
}

// spec: pi-runtime/configuration
#[test]
fn reads_a_filesystem_blob_store_and_resolves_its_path_like_the_others() {
    let config = parse(&blob_store(
        "[blob_store]\nbackend = \"filesystem\"\npath = \"/mnt/vyajena-blobs\"",
    ))
    .check("valid config");
    // An absolute path is the stable path, the same on every start.
    assert_eq!(
        config.blob_store,
        Some(BlobStoreConfig::Filesystem {
            path: "/mnt/vyajena-blobs".into()
        })
    );
    // A relative one is read from the config file's folder.
    let config = parse(&blob_store(
        "[blob_store]\nbackend = \"filesystem\"\npath = \"blobs\"",
    ))
    .check("valid config");
    assert_eq!(
        config.blob_store,
        Some(BlobStoreConfig::Filesystem {
            path: "/srv/pi/blobs".into()
        })
    );
}

// spec: pi-runtime/configuration
#[test]
fn a_bad_blob_store_names_the_field() {
    let cases = [
        ("[blob_store]\npath = \"/mnt/b\"", "blob_store.backend"),
        ("[blob_store]\nbackend = \"filesystem\"", "blob_store.path"),
        (
            "[blob_store]\nbackend = \"floppy\"\npath = \"/mnt/b\"",
            "blob_store.backend",
        ),
        (
            "[blob_store]\nbackend = \"filesystem\"\npath = \" \"",
            "blob_store.path",
        ),
        (
            "[blob_store]\nbackend = \"filesystem\"\npath = \"/b\"\nbucket = \"x\"",
            "blob_store.bucket",
        ),
        ("blob_store = \"/mnt/b\"", "blob_store"),
    ];
    for (table, field) in cases {
        match parse(&blob_store(table)) {
            Err(error) => assert_eq!(error.field(), Some(field), "for {table}: {error}"),
            Ok(_) => panic!("config accepted: {table}"),
        }
    }
}
