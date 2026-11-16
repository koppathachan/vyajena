//! pi-runtime: Push configuration. `push.toml` (no secrets) names the
//! credentials folder; the folder holds the FCM service account key and/or one
//! APNs `.p8` key, owner-only (push design D6).
//!
//! ```toml
//! credentials_dir = "/var/lib/vyajena/push"   # relative: from this file's folder
//! proxy = "http://127.0.0.1:8445"             # optional: the egress proxy
//! apns_team_id = "XMVHK3YASX"
//! apns_topic = "com.purplechaamba.vyajena"
//! ```

use std::path::{Path, PathBuf};

use ring::rand::SystemRandom;
use ring::signature::{ECDSA_P256_SHA256_FIXED_SIGNING, EcdsaKeyPair, RsaKeyPair};
use rustls_pki_types::PrivatePkcs8KeyDer;
use rustls_pki_types::pem::PemObject;

/// The service account key's file name in the credentials folder.
pub const FCM_SERVICE_ACCOUNT_FILE: &str = "fcm-service-account.json";

/// APNs keys are `AuthKey_<key id>.p8`, as Apple names the download.
pub const APNS_KEY_PREFIX: &str = "AuthKey_";

const FIELDS: &[&str] = &["credentials_dir", "proxy", "apns_team_id", "apns_topic"];

/// Why push could not be set up; the server refuses to start.
#[derive(Debug, thiserror::Error)]
pub enum PushConfigError {
    /// A file could not be read.
    #[error("cannot read {path}: {source}")]
    Read {
        /// The file or folder.
        path: PathBuf,
        /// The I/O error.
        source: std::io::Error,
    },
    /// `push.toml` is not valid TOML.
    #[error("push config is not valid TOML: {0}")]
    Syntax(#[from] toml::de::Error),
    /// A setting is missing or invalid.
    #[error("push config field `{field}` {reason}")]
    Field {
        /// The field.
        field: String,
        /// What is wrong.
        reason: String,
    },
    /// The folder or a key file is readable by group or others.
    #[error(
        "{path} has mode {mode:o}; push credentials must be owner-only (0700 folder, 0600 files)"
    )]
    Mode {
        /// The file or folder.
        path: PathBuf,
        /// Its permission bits.
        mode: u32,
    },
    /// A key file does not hold a usable key.
    #[error("{path}: {reason}")]
    Key {
        /// The file.
        path: PathBuf,
        /// What is wrong (never the key).
        reason: String,
    },
    /// The HTTP client could not be built.
    #[error("push HTTP client: {0}")]
    Client(String),
}

/// `push.toml`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PushSettings {
    /// The folder with the key files.
    pub credentials_dir: PathBuf,
    /// The egress proxy, `http://host:port`.
    pub proxy: Option<String>,
    /// The Apple Developer team id (the JWT's `iss`).
    pub apns_team_id: String,
    /// The app's bundle id (`apns-topic`).
    pub apns_topic: String,
}

impl PushSettings {
    /// Reads and checks `push.toml`.
    pub fn load(path: &Path) -> Result<Self, PushConfigError> {
        let text = std::fs::read_to_string(path).map_err(|source| PushConfigError::Read {
            path: path.to_path_buf(),
            source,
        })?;
        Self::from_toml(&text, path.parent().unwrap_or_else(|| Path::new(".")))
    }

    /// Parses and checks `push.toml` text; relative paths are from `base`.
    pub fn from_toml(text: &str, base: &Path) -> Result<Self, PushConfigError> {
        let table: toml::Table = text.parse()?;
        if let Some(unknown) = table.keys().find(|k| !FIELDS.contains(&k.as_str())) {
            return Err(field(unknown, "is unknown"));
        }
        let get = |name: &str| -> Result<Option<String>, PushConfigError> {
            match table.get(name) {
                None => Ok(None),
                Some(toml::Value::String(v)) if v.trim().is_empty() => {
                    Err(field(name, "must not be empty"))
                }
                Some(toml::Value::String(v)) => Ok(Some(v.clone())),
                Some(other) => Err(field(
                    name,
                    &format!("must be a string, found {}", other.type_str()),
                )),
            }
        };
        let required = |name: &str| get(name)?.ok_or_else(|| field(name, "is missing"));

        let credentials_dir = base.join(required("credentials_dir")?);
        let proxy = get("proxy")?;
        if let Some(proxy) = &proxy {
            let rest = proxy
                .strip_prefix("http://")
                .ok_or_else(|| field("proxy", "must be http://host:port"))?;
            if rest
                .rsplit_once(':')
                .is_none_or(|(h, p)| h.is_empty() || p.parse::<u16>().is_err())
            {
                return Err(field("proxy", "must be http://host:port"));
            }
        }
        let apns_team_id = required("apns_team_id")?;
        if apns_team_id.len() != 10 || !apns_team_id.bytes().all(|b| b.is_ascii_alphanumeric()) {
            return Err(field("apns_team_id", "must be 10 letters or digits"));
        }
        let apns_topic = required("apns_topic")?;
        if !apns_topic
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'-')
        {
            return Err(field("apns_topic", "must be a bundle id"));
        }
        Ok(Self {
            credentials_dir,
            proxy,
            apns_team_id,
            apns_topic,
        })
    }
}

fn field(name: &str, reason: &str) -> PushConfigError {
    PushConfigError::Field {
        field: name.to_owned(),
        reason: reason.to_owned(),
    }
}

/// A Google service account key (`fcm-service-account.json`), parsed.
pub struct FcmServiceAccount {
    /// The GCP project (`projects/<id>/messages:send`).
    pub project_id: String,
    /// The JWT's `iss`.
    pub client_email: String,
    /// The JWT's `kid`.
    pub private_key_id: Option<String>,
    /// Where the JWT is exchanged for an access token (and its `aud`).
    pub token_uri: String,
    pub(crate) key: RsaKeyPair,
}

impl std::fmt::Debug for FcmServiceAccount {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FcmServiceAccount")
            .field("project_id", &self.project_id)
            .field("client_email", &self.client_email)
            .finish_non_exhaustive()
    }
}

#[derive(serde::Deserialize)]
struct ServiceAccountFile {
    #[serde(rename = "type")]
    kind: String,
    project_id: String,
    private_key_id: Option<String>,
    private_key: String,
    client_email: String,
    token_uri: Option<String>,
}

impl FcmServiceAccount {
    /// Parses a service account key file's JSON.
    pub fn from_json(json: &[u8], path: &Path) -> Result<Self, PushConfigError> {
        let bad = |reason: &str| PushConfigError::Key {
            path: path.to_path_buf(),
            reason: reason.to_owned(),
        };
        let file: ServiceAccountFile = serde_json::from_slice(json)
            .map_err(|_| bad("not a Google service account key (JSON)"))?;
        if file.kind != "service_account" {
            return Err(bad("type is not service_account"));
        }
        let der = PrivatePkcs8KeyDer::from_pem_slice(file.private_key.as_bytes())
            .map_err(|_| bad("private_key is not a PKCS#8 PEM key"))?;
        let key = RsaKeyPair::from_pkcs8(der.secret_pkcs8_der())
            .map_err(|_| bad("private_key is not a usable RSA key"))?;
        Ok(Self {
            project_id: file.project_id,
            client_email: file.client_email,
            private_key_id: file.private_key_id,
            token_uri: file
                .token_uri
                .unwrap_or_else(|| "https://oauth2.googleapis.com/token".into()),
            key,
        })
    }
}

/// An APNs auth key (`AuthKey_<key id>.p8`), parsed.
pub struct ApnsKey {
    /// The JWT's `kid`, from the file name.
    pub key_id: String,
    pub(crate) key: EcdsaKeyPair,
}

impl std::fmt::Debug for ApnsKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ApnsKey")
            .field("key_id", &self.key_id)
            .finish_non_exhaustive()
    }
}

impl ApnsKey {
    /// Parses a `.p8` (PKCS#8 PEM, P-256) key.
    pub fn from_pem(key_id: &str, pem: &[u8], path: &Path) -> Result<Self, PushConfigError> {
        let bad = |reason: &str| PushConfigError::Key {
            path: path.to_path_buf(),
            reason: reason.to_owned(),
        };
        if key_id.len() != 10 || !key_id.bytes().all(|b| b.is_ascii_alphanumeric()) {
            return Err(bad(
                "the key id in the name AuthKey_<key id>.p8 must be 10 letters or digits",
            ));
        }
        let der =
            PrivatePkcs8KeyDer::from_pem_slice(pem).map_err(|_| bad("not a PKCS#8 PEM key"))?;
        let key = EcdsaKeyPair::from_pkcs8(
            &ECDSA_P256_SHA256_FIXED_SIGNING,
            der.secret_pkcs8_der(),
            &SystemRandom::new(),
        )
        .map_err(|_| bad("not a usable P-256 key"))?;
        Ok(Self {
            key_id: key_id.to_owned(),
            key,
        })
    }
}

/// The key files found in the credentials folder.
#[derive(Debug, Default)]
pub struct Credentials {
    /// From `fcm-service-account.json`, if present.
    pub fcm: Option<FcmServiceAccount>,
    /// From the one `AuthKey_*.p8`, if present.
    pub apns: Option<ApnsKey>,
}

impl Credentials {
    /// Reads the folder: owner-only modes, at most one `.p8`, keys that parse.
    /// A missing key file (or folder) only leaves its platform on the logging
    /// provider.
    pub fn load(dir: &Path) -> Result<Self, PushConfigError> {
        let read_err = |path: &Path| {
            let path = path.to_path_buf();
            move |source| PushConfigError::Read { path, source }
        };
        if !dir.exists() {
            // No credentials yet: every platform only logs (the start log
            // and deploy/pi/check.sh say so).
            return Ok(Self::default());
        }
        owner_only(dir)?;
        let mut apns_files = Vec::new();
        for entry in std::fs::read_dir(dir).map_err(read_err(dir))? {
            let entry = entry.map_err(read_err(dir))?;
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if name.starts_with(APNS_KEY_PREFIX) && name.ends_with(".p8") {
                apns_files.push(entry.path());
            }
        }
        if apns_files.len() > 1 {
            return Err(PushConfigError::Key {
                path: dir.to_path_buf(),
                reason: format!("{} AuthKey_*.p8 files; keep exactly one", apns_files.len()),
            });
        }

        let fcm_path = dir.join(FCM_SERVICE_ACCOUNT_FILE);
        let fcm = if fcm_path.exists() {
            owner_only(&fcm_path)?;
            let json = std::fs::read(&fcm_path).map_err(read_err(&fcm_path))?;
            Some(FcmServiceAccount::from_json(&json, &fcm_path)?)
        } else {
            None
        };
        let apns = match apns_files.pop() {
            Some(path) => {
                owner_only(&path)?;
                let name = path
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default();
                let key_id = name
                    .strip_prefix(APNS_KEY_PREFIX)
                    .and_then(|n| n.strip_suffix(".p8"))
                    .unwrap_or_default()
                    .to_owned();
                let pem = std::fs::read(&path).map_err(read_err(&path))?;
                Some(ApnsKey::from_pem(&key_id, &pem, &path)?)
            }
            None => None,
        };
        Ok(Self { fcm, apns })
    }
}

#[cfg(unix)]
fn owner_only(path: &Path) -> Result<(), PushConfigError> {
    use std::os::unix::fs::PermissionsExt;
    let meta = std::fs::metadata(path).map_err(|source| PushConfigError::Read {
        path: path.to_path_buf(),
        source,
    })?;
    let mode = meta.permissions().mode() & 0o777;
    if mode & 0o077 != 0 {
        return Err(PushConfigError::Mode {
            path: path.to_path_buf(),
            mode,
        });
    }
    Ok(())
}

#[cfg(not(unix))]
fn owner_only(_path: &Path) -> Result<(), PushConfigError> {
    Ok(())
}
