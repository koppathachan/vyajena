//! pi-runtime: Configuration.

mod support;

use std::path::Path;
use std::process::Command;

use server::runtime::{Config, ConfigError};

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
