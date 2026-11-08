//! pi-runtime: Configuration. One TOML file, read at start; a missing or invalid
//! setting stops the server with an error naming the field.
//!
//! ```toml
//! server_id = "pi"                       # the `<server_id>` in every user id
//! admin_username = "akhil"               # the admin is `<admin_username>@<server_id>`
//! listen_address = "127.0.0.1:8443"      # where the Pi listens (TLS)
//! public_address = "relay.example:443"   # optional: put in invite payloads
//! database_path = "data/pi.sqlite"       # relative paths: from the config file's folder
//! key_dir = "data/keys"
//! ```

use std::net::SocketAddr;
use std::path::{Path, PathBuf};

/// The server's settings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    /// This Pi's server id: the part after `@` in every user id.
    pub server_id: String,
    /// The admin's username; the admin is `<admin_username>@<server_id>`.
    pub admin_username: String,
    /// The TCP address the TLS listener binds to.
    pub listen_address: SocketAddr,
    /// `host:port` clients dial (the relay), written into invite payloads.
    /// Defaults to `listen_address`.
    pub public_address: String,
    /// The SQLite database file.
    pub database_path: PathBuf,
    /// The folder holding the TLS and MLS external sender private keys.
    pub key_dir: PathBuf,
}

/// The config field names, as written in the TOML file.
pub mod field {
    /// `server_id`
    pub const SERVER_ID: &str = "server_id";
    /// `admin_username`
    pub const ADMIN_USERNAME: &str = "admin_username";
    /// `listen_address`
    pub const LISTEN_ADDRESS: &str = "listen_address";
    /// `public_address`
    pub const PUBLIC_ADDRESS: &str = "public_address";
    /// `database_path`
    pub const DATABASE_PATH: &str = "database_path";
    /// `key_dir`
    pub const KEY_DIR: &str = "key_dir";

    pub(super) const ALL: &[&str] = &[
        SERVER_ID,
        ADMIN_USERNAME,
        LISTEN_ADDRESS,
        PUBLIC_ADDRESS,
        DATABASE_PATH,
        KEY_DIR,
    ];
}

/// Why the config was refused.
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    /// The file could not be read.
    #[error("cannot read config file {path}: {source}")]
    Read {
        /// The config file.
        path: PathBuf,
        /// The I/O error.
        source: std::io::Error,
    },
    /// The file is not valid TOML.
    #[error("config file is not valid TOML: {0}")]
    Syntax(#[from] toml::de::Error),
    /// A required setting is absent.
    #[error("config field `{field}` is missing")]
    Missing {
        /// The field name.
        field: &'static str,
    },
    /// A setting has the wrong type or an invalid value.
    #[error("config field `{field}` is invalid: {reason}")]
    Invalid {
        /// The field name.
        field: &'static str,
        /// What is wrong.
        reason: String,
    },
    /// A setting the server does not know (often a typo).
    #[error("config field `{field}` is unknown")]
    Unknown {
        /// The field name as written.
        field: String,
    },
}

impl ConfigError {
    /// The field the error is about, if any.
    pub fn field(&self) -> Option<&str> {
        match self {
            Self::Missing { field } | Self::Invalid { field, .. } => Some(field),
            Self::Unknown { field } => Some(field),
            Self::Read { .. } | Self::Syntax(_) => None,
        }
    }
}

impl Config {
    /// Reads and checks a config file. Relative paths in it are resolved from
    /// the file's folder, so the server can be started from anywhere.
    pub fn load(path: &Path) -> Result<Self, ConfigError> {
        let text = std::fs::read_to_string(path).map_err(|source| ConfigError::Read {
            path: path.to_path_buf(),
            source,
        })?;
        let base = path.parent().unwrap_or_else(|| Path::new("."));
        Self::from_toml(&text, base)
    }

    /// Parses and checks config text. Relative paths are resolved from `base`.
    pub fn from_toml(text: &str, base: &Path) -> Result<Self, ConfigError> {
        let table: toml::Table = text.parse()?;
        if let Some(unknown) = table.keys().find(|key| !field::ALL.contains(&key.as_str())) {
            return Err(ConfigError::Unknown {
                field: unknown.clone(),
            });
        }

        let server_id = required_str(&table, field::SERVER_ID)?;
        check_user_id_part(field::SERVER_ID, &server_id)?;
        let admin_username = required_str(&table, field::ADMIN_USERNAME)?;
        check_user_id_part(field::ADMIN_USERNAME, &admin_username)?;

        let listen_text = required_str(&table, field::LISTEN_ADDRESS)?;
        let listen_address: SocketAddr = listen_text.parse().map_err(|_| ConfigError::Invalid {
            field: field::LISTEN_ADDRESS,
            reason: format!("{listen_text:?} is not an IP address and port"),
        })?;

        let public_address = match optional_str(&table, field::PUBLIC_ADDRESS)? {
            Some(address) => {
                check_host_port(field::PUBLIC_ADDRESS, &address)?;
                address
            }
            None => listen_address.to_string(),
        };

        let database_path = base.join(required_str(&table, field::DATABASE_PATH)?);
        let key_dir = base.join(required_str(&table, field::KEY_DIR)?);

        Ok(Self {
            server_id,
            admin_username,
            listen_address,
            public_address,
            database_path,
            key_dir,
        })
    }

    /// The admin's user id in its textual form, `<admin_username>@<server_id>`.
    pub fn admin_user_id(&self) -> String {
        format!("{}@{}", self.admin_username, self.server_id)
    }
}

fn optional_str(table: &toml::Table, field: &'static str) -> Result<Option<String>, ConfigError> {
    match table.get(field) {
        None => Ok(None),
        Some(toml::Value::String(value)) if value.trim().is_empty() => Err(ConfigError::Invalid {
            field,
            reason: "must not be empty".into(),
        }),
        Some(toml::Value::String(value)) => Ok(Some(value.clone())),
        Some(other) => Err(ConfigError::Invalid {
            field,
            reason: format!("must be a string, found {}", other.type_str()),
        }),
    }
}

fn required_str(table: &toml::Table, field: &'static str) -> Result<String, ConfigError> {
    optional_str(table, field)?.ok_or(ConfigError::Missing { field })
}

/// A username or server id: one side of `<username>@<server_id>`.
fn check_user_id_part(field: &'static str, value: &str) -> Result<(), ConfigError> {
    if value.contains('@') {
        return Err(ConfigError::Invalid {
            field,
            reason: "must not contain '@'".into(),
        });
    }
    if value.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Err(ConfigError::Invalid {
            field,
            reason: "must not contain spaces or control characters".into(),
        });
    }
    Ok(())
}

fn check_host_port(field: &'static str, value: &str) -> Result<(), ConfigError> {
    let invalid = || ConfigError::Invalid {
        field,
        reason: format!("{value:?} is not host:port"),
    };
    let (host, port) = value.rsplit_once(':').ok_or_else(invalid)?;
    if host.is_empty() || host.chars().any(char::is_whitespace) || port.parse::<u16>().is_err() {
        return Err(invalid());
    }
    Ok(())
}
