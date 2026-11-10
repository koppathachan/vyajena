//! The opt-in key file (design D3): one file with mode 0600 holding every key,
//! for a machine without an OS keychain.
//!
//! Format: the magic line `vyajena2-keys-v1\n`, then per key one byte of name
//! length, the name ([`KeyName::as_str`]), a 4-byte big-endian secret length
//! and the secret. Every change rewrites the file through a 0600 temporary
//! file, `fsync` and `rename`, so a crash leaves the old or the new file.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use parking_lot::Mutex;
use zeroize::Zeroizing;

use super::{KeyName, KeyStore, KeyStoreError, KeyStoreKind, Secret, io_error};

/// Mode of the key file.
pub const KEY_FILE_MODE: u32 = 0o600;

const MAGIC: &[u8] = b"vyajena2-keys-v1\n";

/// The user's explicit choice to keep keys in a file. Create one only from
/// an explicit user action (a command-line flag or a setting the user turned
/// on), never as a default: it is what lets the core use a file at all
/// (client-runtime: Key storage).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyFileOptIn {
    path: PathBuf,
}

impl KeyFileOptIn {
    /// The user opted in to keeping keys in `path`.
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    /// The key file.
    pub fn path(&self) -> &Path {
        &self.path
    }
}

/// Keys in a 0600 file.
pub struct FileKeyStore {
    path: PathBuf,
    keys: Mutex<BTreeMap<KeyName, Secret>>,
}

impl std::fmt::Debug for FileKeyStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let names: Vec<KeyName> = self.keys.lock().keys().copied().collect();
        f.debug_struct("FileKeyStore")
            .field("path", &self.path)
            .field("keys", &names)
            .finish()
    }
}

impl FileKeyStore {
    /// Opens the opted-in key file, reading it if it exists. Refuses a file
    /// that others can read or write.
    pub fn open(opt_in: &KeyFileOptIn) -> Result<Self, KeyStoreError> {
        let path = opt_in.path().to_path_buf();
        let keys = match read_file(&path)? {
            Some(bytes) => {
                decode(&bytes).ok_or_else(|| KeyStoreError::CorruptFile { path: path.clone() })?
            }
            None => BTreeMap::new(),
        };
        Ok(Self {
            path,
            keys: Mutex::new(keys),
        })
    }

    /// The key file.
    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl KeyStore for FileKeyStore {
    fn kind(&self) -> KeyStoreKind {
        KeyStoreKind::File
    }

    fn get(&self, name: KeyName) -> Result<Option<Secret>, KeyStoreError> {
        Ok(self.keys.lock().get(&name).cloned())
    }

    fn set(&self, name: KeyName, secret: &[u8]) -> Result<(), KeyStoreError> {
        let mut keys = self.keys.lock();
        let mut next = keys.clone();
        next.insert(name, Zeroizing::new(secret.to_vec()));
        write_file(&self.path, &encode(&next))?;
        *keys = next;
        Ok(())
    }

    fn delete(&self, name: KeyName) -> Result<(), KeyStoreError> {
        let mut keys = self.keys.lock();
        if !keys.contains_key(&name) {
            return Ok(());
        }
        let mut next = keys.clone();
        next.remove(&name);
        write_file(&self.path, &encode(&next))?;
        *keys = next;
        Ok(())
    }
}

fn encode(keys: &BTreeMap<KeyName, Secret>) -> Zeroizing<Vec<u8>> {
    let mut out = Zeroizing::new(MAGIC.to_vec());
    for (name, secret) in keys {
        let name = name.as_str().as_bytes();
        // Names are short constants and secrets are keys, so both fit.
        out.push(u8::try_from(name.len()).unwrap_or(u8::MAX));
        out.extend_from_slice(name);
        out.extend_from_slice(
            &u32::try_from(secret.len())
                .unwrap_or(u32::MAX)
                .to_be_bytes(),
        );
        out.extend_from_slice(secret);
    }
    out
}

fn decode(bytes: &[u8]) -> Option<BTreeMap<KeyName, Secret>> {
    let mut rest = bytes.strip_prefix(MAGIC)?;
    let mut keys = BTreeMap::new();
    while let Some((&name_len, after)) = rest.split_first() {
        let (name, after) = after.split_at_checked(usize::from(name_len))?;
        let name = KeyName::parse(std::str::from_utf8(name).ok()?)?;
        let (len, after) = after.split_at_checked(4)?;
        let len = usize::try_from(u32::from_be_bytes(len.try_into().ok()?)).ok()?;
        let (secret, after) = after.split_at_checked(len)?;
        if keys.insert(name, Zeroizing::new(secret.to_vec())).is_some() {
            return None;
        }
        rest = after;
    }
    Some(keys)
}

#[cfg(unix)]
fn read_file(path: &Path) -> Result<Option<Zeroizing<Vec<u8>>>, KeyStoreError> {
    use std::os::unix::fs::PermissionsExt;

    let metadata = match std::fs::metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(io_error(path)(error)),
    };
    let mode = metadata.permissions().mode() & 0o777;
    if mode & 0o077 != 0 {
        return Err(KeyStoreError::InsecureFile {
            path: path.to_path_buf(),
            mode,
        });
    }
    let bytes = std::fs::read(path).map_err(io_error(path))?;
    Ok(Some(Zeroizing::new(bytes)))
}

#[cfg(unix)]
fn write_file(path: &Path, bytes: &[u8]) -> Result<(), KeyStoreError> {
    use std::fs::{DirBuilder, File, OpenOptions};
    use std::io::Write;
    use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};

    let dir = path
        .parent()
        .filter(|dir| !dir.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(dir)
        .map_err(io_error(dir))?;

    let mut tmp_name = path.as_os_str().to_owned();
    tmp_name.push(".tmp");
    let tmp = PathBuf::from(tmp_name);
    match std::fs::remove_file(&tmp) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(io_error(&tmp)(error)),
    }
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(KEY_FILE_MODE)
        .open(&tmp)
        .map_err(io_error(&tmp))?;
    file.write_all(bytes).map_err(io_error(&tmp))?;
    file.sync_all().map_err(io_error(&tmp))?;
    drop(file);
    std::fs::rename(&tmp, path).map_err(io_error(path))?;
    File::open(dir)
        .and_then(|dir| dir.sync_all())
        .map_err(io_error(dir))?;
    Ok(())
}

#[cfg(not(unix))]
fn read_file(_path: &Path) -> Result<Option<Zeroizing<Vec<u8>>>, KeyStoreError> {
    Err(KeyStoreError::Unsupported)
}

#[cfg(not(unix))]
fn write_file(_path: &Path, _bytes: &[u8]) -> Result<(), KeyStoreError> {
    Err(KeyStoreError::Unsupported)
}
