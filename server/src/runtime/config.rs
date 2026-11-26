//! server-runtime: Configuration. One TOML file, read at start; a missing or invalid
//! setting stops the server with an error naming the field.
//!
//! ```toml
//! server_id = "chat"                       # the `<server_id>` in every user id
//! admin_username = "akhil"               # the admin is `<admin_username>@<server_id>`
//! listen_address = "127.0.0.1:8443"      # where the server listens (TLS)
//! public_address = "relay.example:443"   # optional: put in invite payloads
//! database_path = "data/server.sqlite"       # relative paths: from the config file's folder
//! key_dir = "data/keys"
//!
//! [blob_store]                           # optional: without it, images are off
//! backend = "filesystem"
//! path = "/mnt/vyajena-blobs"            # a stable path, the same on every start
//!
//! [recovery]                             # optional: break-glass admin reset
//! passphrase = "..."                     # if set, `server admin-reset` also needs
//!                                        #   this passphrase (defense-in-depth on
//!                                        #   top of host access; admin-recovery).
//!
//! [onboarding]                           # optional: DEV/DEMO ONLY. Absent, or
//!                                        #   mode = "invite", is today's
//!                                        #   invite-only flow. Do NOT enable
//!                                        #   "open" in production.
//! mode = "open"                          # "invite" (default) or "open": open lets
//!                                        #   a device self-register with a chosen
//!                                        #   username, no admin invite. Logs a
//!                                        #   startup WARNING when open.
//! join_code = "letmein"                  # optional shared secret open signup must
//!                                        #   carry; authorises creating an account
//!                                        #   only, never an identity. Never logged.
//! max_users = 10                         # optional cap on active users (both
//!                                        #   modes); new signups past it are
//!                                        #   refused with RESOURCE_EXHAUSTED.
//!
//! [limits]                               # optional: every field defaults to the
//!                                        # built-in value, so an absent table or
//!                                        # field keeps today's behaviour. Durations
//!                                        # are whole seconds; applied on restart.
//! inactivity_timeout = 604800            # 7 days: idle before a device is removed.
//!                                        #   raise (e.g. 2592000 = 30 days) so a
//!                                        #   backgrounded phone is not dropped; the
//!                                        #   trade-off is inactive leaves linger in
//!                                        #   the MLS tree. See also queue_entry_ttl.
//! queue_entry_ttl = 604800               # 7 days: a returning device only gets
//!                                        #   messages newer than this; raise it with
//!                                        #   inactivity_timeout to keep more, at the
//!                                        #   cost of server storage.
//! queue_cap = 1000                       # per-device queue before marked inactive.
//! stream_idle_timeout = 90               # seconds idle before a stream is reaped.
//! invite_ttl = 604800                    # 7 days.
//! blob_ttl = 604800                      # 7 days.
//! challenge_ttl = 60                     # seconds an auth challenge is valid.
//! dedup_min_retention = 604800           # 7 days; keep >= queue_entry_ttl.
//! wake_ping_min_interval = 60            # min seconds between wake pushes per device.
//! catch_up_max = 5                       # bounded catch-up push attempts per backlog.
//! sweep_interval = 60                    # seconds between periodic sweeps.
//! max_blob_store_bytes = 1073741824      # 1 GiB blob-store ceiling.
//! max_uploads_per_device_per_hour = 60
//! max_video_blob_bytes = 5242880         # the one blob cap (video-messages);
//!                                        #   clamped into [image cap, 8 MiB];
//!                                        #   default 5 MiB. Edit + restart, no
//!                                        #   rebuild. A 30 s clip is about 4 MB.
//! ```
//!
//! The effective configuration (all limits and the non-secret base settings) is
//! logged once at startup, so the operator can see what is in force.

use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// The server's settings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    /// This server's server id: the part after `@` in every user id.
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
    /// Where encrypted image blobs are kept; `None` turns images off
    /// (blob-store: Pluggable storage backend).
    pub blob_store: Option<BlobStoreConfig>,
    /// Operator-tunable, server-enforced limits (server-runtime: Configuration).
    /// Every field defaults to the built-in value, so an absent `[limits]`
    /// table keeps today's behaviour.
    pub limits: Limits,
    /// Break-glass admin-reset settings (admin-recovery: Reset needs genuine
    /// host access). An absent `[recovery]` table means host access alone
    /// authorises `server admin-reset`.
    pub recovery: Recovery,
    /// Dev/demo onboarding settings (server-runtime: Dev onboarding mode). An
    /// absent `[onboarding]` table, or `mode = "invite"`, keeps today's
    /// invite-only onboarding exactly.
    pub onboarding: Onboarding,
}

/// How a new user joins this server (server-runtime: Dev onboarding mode).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum OnboardingMode {
    /// Invite-only: a new user needs an admin-minted single-use invite. Today's
    /// behaviour and the default.
    #[default]
    Invite,
    /// Open/dev mode: a device may self-register with a chosen username and no
    /// invite (device-identity: Open registration), bounded by `max_users` and
    /// gated by `join_code` if one is set. Off in production.
    Open,
}

/// Dev/demo onboarding settings. An absent `[onboarding]` table defaults to
/// invite-only with no join code and no user cap, so behaviour is today's.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Onboarding {
    /// The onboarding mode.
    pub mode: OnboardingMode,
    /// The shared registration secret for open mode, if one is required
    /// (device-identity: Join code). It authorises creating an account and is
    /// never an identity credential; it is never logged.
    pub join_code: Option<String>,
    /// The maximum number of active users the server will create, or `None`
    /// for no cap (device-identity: Maximum users). Enforced in both modes.
    pub max_users: Option<u32>,
}

/// Optional defense-in-depth for the break-glass admin reset. When
/// `passphrase` is set, `server admin-reset` additionally requires that exact
/// passphrase; absent, filesystem access to the data directory is the only
/// authority (admin-recovery: Reset needs genuine host access).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Recovery {
    /// The operator passphrase `admin-reset` must be given, if any.
    pub passphrase: Option<String>,
}

/// The server-enforced, operator-tunable limits. Each field defaults to the
/// built-in constant; `[limits]` in the TOML overrides individual values,
/// applied on restart. Client-side timings are not configurable here.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    /// Idle before a member device is removed (`INACTIVITY_TIMEOUT`).
    pub inactivity_timeout: Duration,
    /// How long an undelivered queued entry is kept (`QUEUE_ENTRY_TTL`).
    pub queue_entry_ttl: Duration,
    /// Per-device queue length before the device is marked inactive (`QUEUE_CAP`).
    pub queue_cap: usize,
    /// Idle before a `Connect` stream is reaped (`STREAM_IDLE_TIMEOUT`).
    pub stream_idle_timeout: Duration,
    /// How long a created invite is usable (`INVITE_TTL`).
    pub invite_ttl: Duration,
    /// How long a stored blob is kept (`BLOB_TTL`).
    pub blob_ttl: Duration,
    /// Auth challenge validity (`CHALLENGE_TTL`).
    pub challenge_ttl: Duration,
    /// Minimum retention of dedup records (`DEDUP_MIN_RETENTION`).
    pub dedup_min_retention: Duration,
    /// Minimum interval between wake pushes to one device (`WAKE_PING_MIN_INTERVAL`).
    pub wake_ping_min_interval: Duration,
    /// Bounded catch-up push attempts for one backlog (`CATCH_UP_MAX`).
    pub catch_up_max: u32,
    /// How often the periodic sweep runs (`SWEEP_INTERVAL`, server-local).
    pub sweep_interval: Duration,
    /// Blob-store disk ceiling in bytes (`MAX_BLOB_STORE_BYTES`).
    pub max_blob_store_bytes: u64,
    /// Blob uploads allowed per device per hour (`MAX_UPLOADS_PER_DEVICE_PER_HOUR`).
    pub max_uploads_per_device_per_hour: u32,
    /// The one maximum blob ciphertext size the server enforces on every
    /// `Blobs.UploadBlob` and advertises in `ServerInfo.Limits`
    /// (video-messages: Config-driven limit). The operator's
    /// `max_video_blob_bytes` is clamped into
    /// `[protocol::blob_store::MAX_BLOB_BYTES, protocol::video::MAX_VIDEO_BYTES]`;
    /// an absent value uses `protocol::video::DEFAULT_MAX_VIDEO_BYTES`. Changing
    /// it is a config edit and restart, no rebuild.
    pub max_video_blob_bytes: u64,
}

/// The default sweep interval (server-local; no `protocol` const).
const DEFAULT_SWEEP_INTERVAL: Duration = Duration::from_secs(60);

impl Default for Limits {
    fn default() -> Self {
        Self {
            inactivity_timeout: protocol::group_membership::INACTIVITY_TIMEOUT,
            queue_entry_ttl: protocol::message_delivery::QUEUE_ENTRY_TTL,
            queue_cap: protocol::message_delivery::QUEUE_CAP,
            stream_idle_timeout: protocol::wire_transport::STREAM_IDLE_TIMEOUT,
            invite_ttl: protocol::device_identity::INVITE_TTL,
            blob_ttl: protocol::blob_store::BLOB_TTL,
            challenge_ttl: protocol::wire_transport::CHALLENGE_TTL,
            dedup_min_retention: protocol::message_delivery::DEDUP_MIN_RETENTION,
            wake_ping_min_interval: protocol::message_delivery::WAKE_PING_MIN_INTERVAL,
            catch_up_max: protocol::message_delivery::CATCH_UP_MAX,
            sweep_interval: DEFAULT_SWEEP_INTERVAL,
            max_blob_store_bytes: protocol::blob_store::MAX_BLOB_STORE_BYTES,
            max_uploads_per_device_per_hour: protocol::blob_store::MAX_UPLOADS_PER_DEVICE_PER_HOUR,
            // Unset config: the built-in default (video-messages: Config-driven limit).
            max_video_blob_bytes: protocol::video::clamp_max_blob_bytes(None),
        }
    }
}

/// Which blob-store backend holds the ciphertext, and where.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BlobStoreConfig {
    /// Files in one directory: the stable path the drive is mounted at.
    Filesystem {
        /// The directory, which must already exist; the server never creates it.
        path: PathBuf,
    },
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
    /// `[blob_store]`
    pub const BLOB_STORE: &str = "blob_store";
    /// `blob_store.backend`
    pub const BLOB_STORE_BACKEND: &str = "blob_store.backend";
    /// `blob_store.path`
    pub const BLOB_STORE_PATH: &str = "blob_store.path";
    /// `[limits]`
    pub const LIMITS: &str = "limits";
    /// `[recovery]`
    pub const RECOVERY: &str = "recovery";
    /// `recovery.passphrase`
    pub const RECOVERY_PASSPHRASE: &str = "recovery.passphrase";
    /// `[onboarding]`
    pub const ONBOARDING: &str = "onboarding";
    /// `onboarding.mode`
    pub const ONBOARDING_MODE: &str = "onboarding.mode";
    /// `onboarding.join_code`
    pub const ONBOARDING_JOIN_CODE: &str = "onboarding.join_code";
    /// `onboarding.max_users`
    pub const ONBOARDING_MAX_USERS: &str = "onboarding.max_users";

    pub(super) const ALL: &[&str] = &[
        SERVER_ID,
        ADMIN_USERNAME,
        LISTEN_ADDRESS,
        PUBLIC_ADDRESS,
        DATABASE_PATH,
        KEY_DIR,
        BLOB_STORE,
        LIMITS,
        RECOVERY,
        ONBOARDING,
    ];

    /// The known fields inside the `[limits]` table.
    pub(super) const LIMITS_FIELDS: &[&str] = &[
        "inactivity_timeout",
        "queue_entry_ttl",
        "queue_cap",
        "stream_idle_timeout",
        "invite_ttl",
        "blob_ttl",
        "challenge_ttl",
        "dedup_min_retention",
        "wake_ping_min_interval",
        "catch_up_max",
        "sweep_interval",
        "max_blob_store_bytes",
        "max_uploads_per_device_per_hour",
        protocol::video::CONFIG_KEY,
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
        let blob_store = blob_store(&table, base)?;
        let limits = limits(&table)?;
        let recovery = recovery(&table)?;
        let onboarding = onboarding(&table)?;

        Ok(Self {
            server_id,
            admin_username,
            listen_address,
            public_address,
            database_path,
            key_dir,
            blob_store,
            limits,
            recovery,
            onboarding,
        })
    }

    /// The admin's user id in its textual form, `<admin_username>@<server_id>`.
    pub fn admin_user_id(&self) -> String {
        format!("{}@{}", self.admin_username, self.server_id)
    }
}

/// The `[blob_store]` table, if there is one.
fn blob_store(table: &toml::Table, base: &Path) -> Result<Option<BlobStoreConfig>, ConfigError> {
    let Some(value) = table.get(field::BLOB_STORE) else {
        return Ok(None);
    };
    let toml::Value::Table(sub) = value else {
        return Err(ConfigError::Invalid {
            field: field::BLOB_STORE,
            reason: format!("must be a table, found {}", value.type_str()),
        });
    };
    const KNOWN: &[&str] = &["backend", "path"];
    if let Some(unknown) = sub.keys().find(|key| !KNOWN.contains(&key.as_str())) {
        return Err(ConfigError::Unknown {
            field: format!("{}.{unknown}", field::BLOB_STORE),
        });
    }
    let text = |key: &str, name: &'static str| match sub.get(key) {
        None => Err(ConfigError::Missing { field: name }),
        Some(toml::Value::String(value)) if !value.trim().is_empty() => Ok(value.clone()),
        Some(toml::Value::String(_)) => Err(ConfigError::Invalid {
            field: name,
            reason: "must not be empty".into(),
        }),
        Some(other) => Err(ConfigError::Invalid {
            field: name,
            reason: format!("must be a string, found {}", other.type_str()),
        }),
    };
    match text("backend", field::BLOB_STORE_BACKEND)?.as_str() {
        "filesystem" => Ok(Some(BlobStoreConfig::Filesystem {
            path: base.join(text("path", field::BLOB_STORE_PATH)?),
        })),
        other => Err(ConfigError::Invalid {
            field: field::BLOB_STORE_BACKEND,
            reason: format!("{other:?} is not a known backend (only \"filesystem\")"),
        }),
    }
}

/// The `[limits]` table. Every field is optional and defaults to the built-in
/// value; durations are whole seconds, counts and byte sizes are integers. A
/// present field must be a positive integer within sane bounds.
fn limits(table: &toml::Table) -> Result<Limits, ConfigError> {
    let mut limits = Limits::default();
    let Some(value) = table.get(field::LIMITS) else {
        return Ok(limits);
    };
    let toml::Value::Table(sub) = value else {
        return Err(ConfigError::Invalid {
            field: field::LIMITS,
            reason: format!("must be a table, found {}", value.type_str()),
        });
    };
    if let Some(unknown) = sub
        .keys()
        .find(|k| !field::LIMITS_FIELDS.contains(&k.as_str()))
    {
        return Err(ConfigError::Unknown {
            field: format!("{}.{unknown}", field::LIMITS),
        });
    }

    if let Some(n) = pos_int(sub, "inactivity_timeout")? {
        limits.inactivity_timeout = Duration::from_secs(n);
    }
    if let Some(n) = pos_int(sub, "queue_entry_ttl")? {
        limits.queue_entry_ttl = Duration::from_secs(n);
    }
    if let Some(n) = pos_int(sub, "queue_cap")? {
        limits.queue_cap = usize::try_from(n).unwrap_or(usize::MAX);
    }
    if let Some(n) = pos_int(sub, "stream_idle_timeout")? {
        limits.stream_idle_timeout = Duration::from_secs(n);
    }
    if let Some(n) = pos_int(sub, "invite_ttl")? {
        limits.invite_ttl = Duration::from_secs(n);
    }
    if let Some(n) = pos_int(sub, "blob_ttl")? {
        limits.blob_ttl = Duration::from_secs(n);
    }
    if let Some(n) = pos_int(sub, "challenge_ttl")? {
        limits.challenge_ttl = Duration::from_secs(n);
    }
    if let Some(n) = pos_int(sub, "dedup_min_retention")? {
        limits.dedup_min_retention = Duration::from_secs(n);
    }
    if let Some(n) = pos_int(sub, "wake_ping_min_interval")? {
        limits.wake_ping_min_interval = Duration::from_secs(n);
    }
    if let Some(n) = pos_int(sub, "catch_up_max")? {
        limits.catch_up_max = bounded_u32("catch_up_max", n)?;
    }
    if let Some(n) = pos_int(sub, "sweep_interval")? {
        limits.sweep_interval = Duration::from_secs(n);
    }
    if let Some(n) = pos_int(sub, "max_blob_store_bytes")? {
        let min = protocol::blob_store::MAX_BLOB_BYTES as u64;
        if n < min {
            return Err(ConfigError::Invalid {
                field: "max_blob_store_bytes",
                reason: format!("must be at least one blob ({min} bytes)"),
            });
        }
        limits.max_blob_store_bytes = n;
    }
    if let Some(n) = pos_int(sub, "max_uploads_per_device_per_hour")? {
        limits.max_uploads_per_device_per_hour = bounded_u32("max_uploads_per_device_per_hour", n)?;
    }
    // video-messages: Config-driven limit. A present value is clamped into the
    // safe range rather than rejected (the scenario "Config beyond the ceiling"
    // clamps, and a value below the image cap is raised to it), so a generous
    // config still starts the server.
    if let Some(n) = pos_int(sub, protocol::video::CONFIG_KEY)? {
        limits.max_video_blob_bytes = protocol::video::clamp_max_blob_bytes(Some(n));
    }
    Ok(limits)
}

/// The `[recovery]` table, if there is one. Only `passphrase` is known; it must
/// be a non-empty string when present (admin-recovery: Reset needs genuine host
/// access).
fn recovery(table: &toml::Table) -> Result<Recovery, ConfigError> {
    let Some(value) = table.get(field::RECOVERY) else {
        return Ok(Recovery::default());
    };
    let toml::Value::Table(sub) = value else {
        return Err(ConfigError::Invalid {
            field: field::RECOVERY,
            reason: format!("must be a table, found {}", value.type_str()),
        });
    };
    const KNOWN: &[&str] = &["passphrase"];
    if let Some(unknown) = sub.keys().find(|key| !KNOWN.contains(&key.as_str())) {
        return Err(ConfigError::Unknown {
            field: format!("{}.{unknown}", field::RECOVERY),
        });
    }
    let passphrase = match sub.get("passphrase") {
        None => None,
        Some(toml::Value::String(value)) if !value.is_empty() => Some(value.clone()),
        Some(toml::Value::String(_)) => {
            return Err(ConfigError::Invalid {
                field: field::RECOVERY_PASSPHRASE,
                reason: "must not be empty".into(),
            });
        }
        Some(other) => {
            return Err(ConfigError::Invalid {
                field: field::RECOVERY_PASSPHRASE,
                reason: format!("must be a string, found {}", other.type_str()),
            });
        }
    };
    Ok(Recovery { passphrase })
}

/// The `[onboarding]` table, if there is one (server-runtime: Dev onboarding
/// mode). Every field is optional: `mode` is `"invite"` (default) or `"open"`,
/// `join_code` a non-empty string, `max_users` a positive integer. An absent
/// table keeps today's invite-only onboarding.
fn onboarding(table: &toml::Table) -> Result<Onboarding, ConfigError> {
    let Some(value) = table.get(field::ONBOARDING) else {
        return Ok(Onboarding::default());
    };
    let toml::Value::Table(sub) = value else {
        return Err(ConfigError::Invalid {
            field: field::ONBOARDING,
            reason: format!("must be a table, found {}", value.type_str()),
        });
    };
    const KNOWN: &[&str] = &["mode", "join_code", "max_users"];
    if let Some(unknown) = sub.keys().find(|key| !KNOWN.contains(&key.as_str())) {
        return Err(ConfigError::Unknown {
            field: format!("{}.{unknown}", field::ONBOARDING),
        });
    }
    let mode = match sub.get("mode") {
        None => OnboardingMode::default(),
        Some(toml::Value::String(value)) => match value.as_str() {
            "invite" => OnboardingMode::Invite,
            "open" => OnboardingMode::Open,
            other => {
                return Err(ConfigError::Invalid {
                    field: field::ONBOARDING_MODE,
                    reason: format!("{other:?} is not a known mode (only \"invite\" or \"open\")"),
                });
            }
        },
        Some(other) => {
            return Err(ConfigError::Invalid {
                field: field::ONBOARDING_MODE,
                reason: format!("must be a string, found {}", other.type_str()),
            });
        }
    };
    let join_code = match sub.get("join_code") {
        None => None,
        Some(toml::Value::String(value)) if !value.is_empty() => Some(value.clone()),
        Some(toml::Value::String(_)) => {
            return Err(ConfigError::Invalid {
                field: field::ONBOARDING_JOIN_CODE,
                reason: "must not be empty".into(),
            });
        }
        Some(other) => {
            return Err(ConfigError::Invalid {
                field: field::ONBOARDING_JOIN_CODE,
                reason: format!("must be a string, found {}", other.type_str()),
            });
        }
    };
    let max_users = match sub.get("max_users") {
        None => None,
        Some(toml::Value::Integer(n)) if *n > 0 => {
            Some(bounded_u32(field::ONBOARDING_MAX_USERS, *n as u64)?)
        }
        Some(toml::Value::Integer(_)) => {
            return Err(ConfigError::Invalid {
                field: field::ONBOARDING_MAX_USERS,
                reason: "must be a positive integer".into(),
            });
        }
        Some(other) => {
            return Err(ConfigError::Invalid {
                field: field::ONBOARDING_MAX_USERS,
                reason: format!("must be a positive integer, found {}", other.type_str()),
            });
        }
    };
    Ok(Onboarding {
        mode,
        join_code,
        max_users,
    })
}

/// An optional positive integer from a `[limits]` sub-field, erroring (naming
/// the field) if present but not a positive integer.
fn pos_int(sub: &toml::Table, name: &'static str) -> Result<Option<u64>, ConfigError> {
    match sub.get(name) {
        None => Ok(None),
        Some(toml::Value::Integer(n)) if *n > 0 => Ok(Some(*n as u64)),
        Some(toml::Value::Integer(_)) => Err(ConfigError::Invalid {
            field: name,
            reason: "must be a positive integer".into(),
        }),
        Some(other) => Err(ConfigError::Invalid {
            field: name,
            reason: format!("must be a positive integer, found {}", other.type_str()),
        }),
    }
}

/// A positive integer that must fit in a `u32`.
fn bounded_u32(name: &'static str, n: u64) -> Result<u32, ConfigError> {
    u32::try_from(n).map_err(|_| ConfigError::Invalid {
        field: name,
        reason: format!("must be at most {}", u32::MAX),
    })
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

/// What is wrong with a username or server id (one side of
/// `<username>@<server_id>`), if anything: it must be non-empty and free of
/// `@`, whitespace and control characters. Also used for the usernames the
/// admin picks in `Admin.CreateInvite`.
pub fn user_id_part_problem(value: &str) -> Option<&'static str> {
    if value.is_empty() {
        return Some("must not be empty");
    }
    if value.contains('@') {
        return Some("must not contain '@'");
    }
    if value.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Some("must not contain spaces or control characters");
    }
    None
}

/// A username or server id: one side of `<username>@<server_id>`.
fn check_user_id_part(field: &'static str, value: &str) -> Result<(), ConfigError> {
    match user_id_part_problem(value) {
        Some(reason) => Err(ConfigError::Invalid {
            field,
            reason: reason.into(),
        }),
        None => Ok(()),
    }
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
