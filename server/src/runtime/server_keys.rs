//! pi-runtime: First start. The Pi's two private keys, created on first start
//! and reused afterwards (pi-server-core design D6):
//!
//! - `tls_key.pem`: the TLS key (ECDSA P-256, PKCS#8 PEM). The certificate is
//!   self-signed and rebuilt from it on every start; clients pin only the
//!   SHA-256 of its SubjectPublicKeyInfo, so the pin never changes.
//! - `mls_external_sender_key.pem`: the Ed25519 key (PKCS#8 PEM) the Pi signs
//!   its MLS external Remove proposals with.
//!
//! Both are written with mode 0600 into a key folder created with mode 0700.

use std::fmt;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};

use ed25519_dalek::SigningKey;
use ed25519_dalek::pkcs8::{DecodePrivateKey, EncodePrivateKey, spki::der::pem::LineEnding};
use rcgen::{KeyPair, PKCS_ECDSA_P256_SHA256, PublicKeyData};
use sha2::{Digest, Sha256};

/// File name of the TLS private key in the key folder.
pub const TLS_KEY_FILE: &str = "tls_key.pem";
/// File name of the MLS external sender private key in the key folder.
pub const EXTERNAL_SENDER_KEY_FILE: &str = "mls_external_sender_key.pem";

/// Mode of every private key file.
pub const KEY_FILE_MODE: u32 = 0o600;
/// Mode of a key folder the server creates.
pub const KEY_DIR_MODE: u32 = 0o700;

/// SHA-256 of the TLS SubjectPublicKeyInfo: what clients pin.
pub type SpkiPin = [u8; 32];

/// Why the keys could not be loaded or created.
#[derive(Debug, thiserror::Error)]
pub enum KeyError {
    /// Reading or writing a key file or the key folder failed.
    #[error("{path}: {source}")]
    Io {
        /// The file or folder.
        path: PathBuf,
        /// The I/O error.
        source: std::io::Error,
    },
    /// Only one of the two key files exists. Generating the missing one could
    /// silently replace a key clients or the group already trust, so the
    /// server stops instead.
    #[error("{missing} is missing but {present} exists; restore it or empty the key folder")]
    Incomplete {
        /// The file that exists.
        present: PathBuf,
        /// The file that does not.
        missing: PathBuf,
    },
    /// A key file is readable or writable by other users.
    #[error("{path} has mode {mode:o}; private keys must have mode 600")]
    Permissions {
        /// The key file.
        path: PathBuf,
        /// Its permission bits.
        mode: u32,
    },
    /// A key file does not hold a key of the expected type.
    #[error("{path} is not a valid {kind} private key: {reason}")]
    Invalid {
        /// The key file.
        path: PathBuf,
        /// What it should hold.
        kind: &'static str,
        /// The parser's complaint.
        reason: String,
    },
    /// Key generation failed.
    #[error("generating the {kind} key failed: {reason}")]
    Generate {
        /// Which key.
        kind: &'static str,
        /// The cause.
        reason: String,
    },
}

/// The Pi's private keys, loaded into memory.
pub struct ServerKeys {
    tls_key: KeyPair,
    tls_spki_sha256: SpkiPin,
    external_sender: SigningKey,
    created: bool,
}

impl fmt::Debug for ServerKeys {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ServerKeys")
            .field("tls_spki_sha256", &hex(&self.tls_spki_sha256))
            .field("created", &self.created)
            .finish_non_exhaustive()
    }
}

impl ServerKeys {
    /// Loads both keys from `dir`, or creates both when neither exists.
    pub fn load_or_create(dir: &Path) -> Result<Self, KeyError> {
        ensure_dir(dir)?;
        let tls_path = dir.join(TLS_KEY_FILE);
        let sender_path = dir.join(EXTERNAL_SENDER_KEY_FILE);

        match (exists(&tls_path)?, exists(&sender_path)?) {
            (false, false) => Self::create(dir, &tls_path, &sender_path),
            (true, true) => Self::load(&tls_path, &sender_path),
            (true, false) => Err(KeyError::Incomplete {
                present: tls_path,
                missing: sender_path,
            }),
            (false, true) => Err(KeyError::Incomplete {
                present: sender_path,
                missing: tls_path,
            }),
        }
    }

    fn create(dir: &Path, tls_path: &Path, sender_path: &Path) -> Result<Self, KeyError> {
        let tls_key =
            KeyPair::generate_for(&PKCS_ECDSA_P256_SHA256).map_err(|e| KeyError::Generate {
                kind: "TLS",
                reason: e.to_string(),
            })?;

        let mut seed = [0u8; 32];
        getrandom::fill(&mut seed).map_err(|e| KeyError::Generate {
            kind: "MLS external sender",
            reason: e.to_string(),
        })?;
        let external_sender = SigningKey::from_bytes(&seed);
        seed.fill(0);
        let sender_pem =
            external_sender
                .to_pkcs8_pem(LineEnding::LF)
                .map_err(|e| KeyError::Generate {
                    kind: "MLS external sender",
                    reason: e.to_string(),
                })?;

        write_private(tls_path, tls_key.serialize_pem().as_bytes())?;
        write_private(sender_path, sender_pem.as_bytes())?;
        sync_dir(dir)?;

        Ok(Self::assemble(tls_key, external_sender, true))
    }

    fn load(tls_path: &Path, sender_path: &Path) -> Result<Self, KeyError> {
        let tls_pem = read_private(tls_path)?;
        let tls_key = KeyPair::from_pem(&tls_pem).map_err(|e| KeyError::Invalid {
            path: tls_path.to_path_buf(),
            kind: "TLS",
            reason: e.to_string(),
        })?;
        let sender_pem = read_private(sender_path)?;
        let external_sender =
            SigningKey::from_pkcs8_pem(&sender_pem).map_err(|e| KeyError::Invalid {
                path: sender_path.to_path_buf(),
                kind: "Ed25519",
                reason: e.to_string(),
            })?;
        Ok(Self::assemble(tls_key, external_sender, false))
    }

    fn assemble(tls_key: KeyPair, external_sender: SigningKey, created: bool) -> Self {
        let tls_spki_sha256 = Sha256::digest(tls_key.subject_public_key_info()).into();
        Self {
            tls_key,
            tls_spki_sha256,
            external_sender,
            created,
        }
    }

    /// Whether this call created the keys (first start).
    pub fn created(&self) -> bool {
        self.created
    }

    /// The TLS pin: SHA-256 of the TLS SubjectPublicKeyInfo.
    pub fn tls_spki_sha256(&self) -> SpkiPin {
        self.tls_spki_sha256
    }

    /// The TLS pin as lowercase hex, as logged.
    pub fn tls_spki_sha256_hex(&self) -> String {
        hex(&self.tls_spki_sha256)
    }

    /// The TLS key pair, for building the self-signed certificate.
    pub fn tls_key(&self) -> &KeyPair {
        &self.tls_key
    }

    /// The MLS external sender signing key.
    pub fn external_sender_key(&self) -> &SigningKey {
        &self.external_sender
    }

    /// The MLS external sender public key (32 bytes), as published in
    /// `ServerInfo.external_sender.signature_public_key`.
    pub fn external_sender_public_key(&self) -> [u8; 32] {
        self.external_sender.verifying_key().to_bytes()
    }
}

/// Lowercase hex.
pub fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    bytes
        .iter()
        .fold(String::with_capacity(bytes.len() * 2), |mut s, b| {
            let _ = write!(s, "{b:02x}");
            s
        })
}

fn io_err(path: &Path) -> impl FnOnce(std::io::Error) -> KeyError + '_ {
    move |source| KeyError::Io {
        path: path.to_path_buf(),
        source,
    }
}

fn ensure_dir(dir: &Path) -> Result<(), KeyError> {
    if dir.is_dir() {
        return Ok(());
    }
    fs::DirBuilder::new()
        .recursive(true)
        .mode(KEY_DIR_MODE)
        .create(dir)
        .map_err(io_err(dir))
}

fn exists(path: &Path) -> Result<bool, KeyError> {
    path.try_exists().map_err(io_err(path))
}

fn write_private(path: &Path, contents: &[u8]) -> Result<(), KeyError> {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(KEY_FILE_MODE)
        .open(path)
        .map_err(io_err(path))?;
    // The umask can only remove bits from 0600, but set it exactly anyway.
    file.set_permissions(fs::Permissions::from_mode(KEY_FILE_MODE))
        .map_err(io_err(path))?;
    file.write_all(contents).map_err(io_err(path))?;
    file.sync_all().map_err(io_err(path))
}

fn read_private(path: &Path) -> Result<String, KeyError> {
    let mode = fs::metadata(path)
        .map_err(io_err(path))?
        .permissions()
        .mode()
        & 0o777;
    if mode & 0o077 != 0 {
        return Err(KeyError::Permissions {
            path: path.to_path_buf(),
            mode,
        });
    }
    fs::read_to_string(path).map_err(io_err(path))
}

fn sync_dir(dir: &Path) -> Result<(), KeyError> {
    fs::File::open(dir)
        .and_then(|d| d.sync_all())
        .map_err(io_err(dir))
}
