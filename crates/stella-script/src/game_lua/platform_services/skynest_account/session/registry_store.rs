//! Scoped native registry persistence; never silently replaces damaged input.
//!
//! The caller chooses an explicitly authorized provider-scope path. This
//! adapter never discovers/imports a player's historical fusion.registry.
//! Format compatibility belongs to registry_codec; locking, atomic commits,
//! restrictive creation modes and input limits are explicit host safeguards.

use super::{
    registry_codec::{decode_registry, encode_registry},
    store::{RefreshStore, StoreError},
};
use serde_json::{Map, Value};
use std::{
    collections::HashMap,
    fs::{self, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex, OnceLock, Weak,
        atomic::{AtomicU64, Ordering},
    },
};

const REFRESH_KEY: &str = "RovioIdentityRefreshToken";
const PROFILE_KEY: &str = "CloudUserProfile_default";
const MAX_DOCUMENT_BYTES: u64 = 8 * 1024 * 1024;

pub(in super::super) struct RegistryStore {
    path: PathBuf,
    lock_path: PathBuf,
    process_lock: Arc<Mutex<()>>,
}

/// Shared native registry namespaces, including non-account services. Paths
/// are supplied by the host; this never searches for historical player data.
pub(in crate::game_lua::platform_services) struct RegistryNamespace {
    store: RegistryStore,
    namespace: Vec<String>,
}

impl RegistryNamespace {
    pub(in crate::game_lua::platform_services) fn open(
        path: PathBuf,
        namespace: &[&str],
    ) -> Result<Self, StoreError> {
        Ok(Self {
            store: RegistryStore::open(path)?,
            namespace: namespace.iter().map(|key| (*key).to_owned()).collect(),
        })
    }

    pub(in crate::game_lua::platform_services) fn get(
        &self,
        key: &str,
    ) -> Result<Option<Value>, StoreError> {
        self.store.read(|document| {
            let mut document = document.clone();
            // Native typed has-key queries return false on a non-object
            // namespace leaf; only traversing a non-object parent throws.
            Ok(self.value(&mut document)?.get(key).cloned())
        })
    }

    pub(in crate::game_lua::platform_services) fn set(
        &self,
        key: &str,
        value: Value,
    ) -> Result<(), StoreError> {
        self.store.with_lock(|| {
            let mut document = read_document(&self.store.path)?;
            native_mutable_object(self.value(&mut document)?)?.insert(key.to_owned(), value);
            write_document(&self.store.path, &document)
        })
    }

    fn value<'a>(&self, document: &'a mut Value) -> Result<&'a mut Value, StoreError> {
        let mut value = document;
        for key in &self.namespace {
            value = native_mutable_object(value)?
                .entry(key)
                .or_insert(Value::Null);
        }
        Ok(value)
    }
}

impl RegistryStore {
    pub(in super::super) fn avatar_creation_time(
        &self,
        directory: &str,
    ) -> Result<i64, StoreError> {
        self.read(|document| {
            Ok(document
                .get("SkynestAvatarCreationTime")
                .and_then(|value| value.get(directory))
                .filter(|value| value.is_number())
                .and_then(|value| super::protocol::signed_number(value).ok())
                .unwrap_or(0))
        })
    }

    pub(in super::super) fn set_avatar_creation_time(
        &self,
        directory: &str,
        time: i64,
    ) -> Result<(), StoreError> {
        self.with_lock(|| {
            let mut document = read_document(&self.path)?;
            let root = native_mutable_object(&mut document)?;
            let times = native_mutable_object(
                root.entry("SkynestAvatarCreationTime")
                    .or_insert(Value::Null),
            )?;
            times.insert(directory.to_owned(), Value::from(time));
            write_document(&self.path, &document)
        })
    }
    pub(in super::super) fn installation_id(&self) -> Result<String, StoreError> {
        self.installation_id_with(|| {
            crate::game_lua::platform::generate_uuid_v4().map_err(|_| StoreError::Io)
        })
    }

    fn installation_id_with(
        &self,
        generate: impl FnOnce() -> Result<String, StoreError>,
    ) -> Result<String, StoreError> {
        self.with_lock(|| {
            let mut document = read_document(&self.path)?;
            let root = native_mutable_object(&mut document)?;
            let id = native_mutable_object(root.entry("id").or_insert(Value::Null))?;
            // Native 1007227F8 tests String, not nonempty or UUID syntax.
            if let Some(value) = id.get("accountUUID").and_then(Value::as_str) {
                return Ok(value.to_owned());
            }
            // 100530378 is a second registry lookup, not a fresh UUID call.
            let fusion = native_mutable_object(root.entry("fusion").or_insert(Value::Null))?;
            let value = match fusion.get("installationID").and_then(Value::as_str) {
                Some(value) => value.to_owned(),
                None => {
                    let value = generate()?;
                    fusion.insert("installationID".to_owned(), Value::String(value.clone()));
                    value
                }
            };
            native_mutable_object(root.get_mut("id").expect("validated id object"))?
                .insert("accountUUID".to_owned(), Value::String(value.clone()));
            // Publish only after durable commit; corruption/I/O failures cannot
            // produce an ephemeral success identity. This is a host safeguard.
            write_document(&self.path, &document)?;
            Ok(value)
        })
    }

    pub(in super::super) fn regenerate_account_id(&self) -> Result<String, StoreError> {
        self.regenerate_account_id_with(|| {
            crate::game_lua::platform::generate_uuid_v4().map_err(|_| StoreError::Io)
        })
    }

    fn regenerate_account_id_with(
        &self,
        generate: impl FnOnce() -> Result<String, StoreError>,
    ) -> Result<String, StoreError> {
        self.with_lock(|| {
            let mut document = read_document(&self.path)?;
            // 1007229D8 generates inside the transaction, then replaces only
            // id.accountUUID. It neither reads nor regenerates the fusion id.
            let value = generate()?;
            let root = native_mutable_object(&mut document)?;
            let id = native_mutable_object(root.entry("id").or_insert(Value::Null))?;
            id.insert("accountUUID".to_owned(), Value::String(value.clone()));
            write_document(&self.path, &document)?;
            Ok(value)
        })
    }

    pub(in super::super) fn open(path: PathBuf) -> Result<Self, StoreError> {
        let name = path.file_name().ok_or(StoreError::Io)?.to_owned();
        let parent = path
            .parent()
            .filter(|path| !path.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        fs::create_dir_all(parent).map_err(|_| StoreError::Io)?;
        let path = parent
            .canonicalize()
            .map_err(|_| StoreError::Io)?
            .join(name);
        let lock_path = append_suffix(&path, ".lock");
        let store = Self {
            process_lock: shared_path_lock(&path)?,
            path,
            lock_path,
        };
        // Ciphertext/outer-JSON corruption is reported, not reset to {}. The
        // native empty-input wrapper produces Null; valid non-object JSON is
        // also retained. The registry is not created until an explicit write.
        store.read(|_| Ok(()))?;
        Ok(store)
    }

    fn with_lock<T>(
        &self,
        operation: impl FnOnce() -> Result<T, StoreError>,
    ) -> Result<T, StoreError> {
        let _process = self.process_lock.lock().map_err(|_| StoreError::Io)?;
        reject_non_regular(&self.lock_path)?;
        let mut options = private_options();
        options.read(true).write(true).create(true).truncate(false);
        let lock_file = options.open(&self.lock_path).map_err(|_| StoreError::Io)?;
        if !lock_file.metadata().map_err(|_| StoreError::Io)?.is_file() {
            return Err(StoreError::Io);
        }
        with_file_lock(lock_file, operation)
    }

    fn read<T>(
        &self,
        inspect: impl FnOnce(&Value) -> Result<T, StoreError>,
    ) -> Result<T, StoreError> {
        self.with_lock(|| inspect(&read_document(&self.path)?))
    }

    fn edit(&self, update: impl FnOnce(&mut Map<String, Value>)) -> Result<(), StoreError> {
        self.with_lock(|| {
            // Reload for EVERY transaction, including separate same-path
            // RegistryStore instances. This preserves newly added unknown
            // root/cloud keys instead of saving a stale constructor snapshot.
            let mut document = read_document(&self.path)?;
            let root = native_mutable_object(&mut document)?;
            let cloud = root.entry("cloud").or_insert(Value::Null);
            update(native_mutable_object(cloud)?);
            write_document(&self.path, &document)
        })
    }
}

// Closing a descriptor alone does not release flock while a duplicated or
// fork-inherited descriptor still refers to the same open file description.
// Explicitly unlock before releasing the process mutex, including on errors
// and unwinding. A real competing process still gets the bounded try_lock error.
fn with_file_lock<T>(
    file: fs::File,
    operation: impl FnOnce() -> Result<T, StoreError>,
) -> Result<T, StoreError> {
    file.try_lock().map_err(|_| StoreError::Io)?;
    let mut guard = FileLock { file, held: true };
    let result = operation();
    guard.file.unlock().map_err(|_| StoreError::Io)?;
    guard.held = false;
    result
}

struct FileLock {
    file: fs::File,
    held: bool,
}

impl Drop for FileLock {
    fn drop(&mut self) {
        if self.held {
            // Best effort only during unwinding or after a reported unlock
            // error. Normal completion reports any unlock failure above.
            let _ = self.file.unlock();
        }
    }
}

fn write_document(path: &Path, document: &Value) -> Result<(), StoreError> {
    let plaintext = serde_json::to_vec(document).map_err(|_| StoreError::InvalidDocument)?;
    if plaintext.len() as u64 > MAX_DOCUMENT_BYTES {
        return Err(StoreError::InvalidDocument);
    }
    let encrypted = encode_registry(&plaintext).map_err(|_| StoreError::Io)?;
    atomic_replace(path, &encrypted)
}

impl RefreshStore for RegistryStore {
    fn avatar_directory(&self) -> Option<PathBuf> {
        self.path.parent().map(|parent| parent.join("avatarAssets"))
    }

    fn load_avatar_version(&self, basename: &str) -> Result<String, StoreError> {
        self.read(|document| {
            Ok(cloud_string(document, &format!("avatarAsset#{basename}"))
                .unwrap_or_default()
                .to_owned())
        })
    }

    fn store_avatar_version(&self, basename: &str, version: &str) -> Result<(), StoreError> {
        self.edit(|cloud| {
            cloud.insert(
                format!("avatarAsset#{basename}"),
                Value::String(version.to_owned()),
            );
        })
    }

    fn load(&self) -> Result<String, StoreError> {
        self.read(|document| {
            Ok(cloud_string(document, REFRESH_KEY)
                .unwrap_or_default()
                .to_owned())
        })
    }

    fn store(&self, refresh: &str) -> Result<(), StoreError> {
        self.edit(|cloud| {
            cloud.insert(REFRESH_KEY.to_owned(), Value::String(refresh.to_owned()));
        })
    }

    fn load_profile(&self) -> Result<Option<Value>, StoreError> {
        self.read(read_profile)
    }

    fn store_profile(&self, profile: Option<&Value>) -> Result<(), StoreError> {
        let text = match profile {
            Some(profile) => {
                serde_json::to_string(profile).map_err(|_| StoreError::InvalidDocument)?
            }
            None => String::new(),
        };
        // Native removal stores an empty string; it does not delete the key.
        self.edit(|cloud| {
            cloud.insert(PROFILE_KEY.to_owned(), Value::String(text));
        })
    }
}

fn shared_path_lock(path: &Path) -> Result<Arc<Mutex<()>>, StoreError> {
    static LOCKS: OnceLock<Mutex<HashMap<PathBuf, Weak<Mutex<()>>>>> = OnceLock::new();
    let mut locks = LOCKS
        .get_or_init(Mutex::default)
        .lock()
        .map_err(|_| StoreError::Io)?;
    locks.retain(|_, lock| lock.strong_count() != 0);
    if let Some(lock) = locks.get(path).and_then(Weak::upgrade) {
        return Ok(lock);
    }
    let lock = Arc::new(Mutex::new(()));
    locks.insert(path.to_owned(), Arc::downgrade(&lock));
    Ok(lock)
}

fn read_document(path: &Path) -> Result<Value, StoreError> {
    reject_non_regular(path)?;
    let mut options = private_options();
    options.read(true);
    let file = match options.open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Value::Null),
        Err(_) => return Err(StoreError::Io),
    };
    let mut encrypted = Vec::new();
    // The codec has no header/IV prefix, only at most one padding block.
    file.take(MAX_DOCUMENT_BYTES + 17)
        .read_to_end(&mut encrypted)
        .map_err(|_| StoreError::Io)?;
    if encrypted.len() as u64 > MAX_DOCUMENT_BYTES + 16 {
        return Err(StoreError::InvalidDocument);
    }
    let plaintext = decode_registry(&encrypted).map_err(|_| StoreError::InvalidCiphertext)?;
    if plaintext.len() as u64 > MAX_DOCUMENT_BYTES {
        return Err(StoreError::InvalidDocument);
    }
    if plaintext.is_empty() {
        return Ok(Value::Null);
    }
    serde_json::from_slice(&plaintext).map_err(|_| StoreError::InvalidDocument)
}

fn cloud_string<'a>(document: &'a Value, key: &str) -> Option<&'a str> {
    // 1006FCA60 uses typed Object/String predicates. A valid wrong-type
    // root/cloud/leaf is an empty getter result, not malformed registry data.
    document.get("cloud")?.get(key)?.as_str()
}

fn read_profile(document: &Value) -> Result<Option<Value>, StoreError> {
    match cloud_string(document, PROFILE_KEY) {
        None | Some("") => Ok(None),
        Some(text) => serde_json::from_str(text)
            .map(Some)
            .map_err(|_| StoreError::InvalidDocument),
    }
}

fn native_mutable_object(value: &mut Value) -> Result<&mut Map<String, Value>, StoreError> {
    // 10055C794 upgrades Null to Object, but checks/throws for every other
    // non-object type. This applies both to the root and root["cloud"]. The
    // final registry String assignment may replace any existing leaf type.
    if value.is_null() {
        *value = Value::Object(Map::new());
    }
    value.as_object_mut().ok_or(StoreError::InvalidDocument)
}

fn reject_non_regular(path: &Path) -> Result<(), StoreError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_file() => Ok(()),
        Ok(_) => Err(StoreError::Io),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(_) => Err(StoreError::Io),
    }
}

pub(super) fn private_options() -> OpenOptions {
    #[allow(unused_mut)]
    let mut options = OpenOptions::new();
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600).custom_flags(libc::O_NOFOLLOW);
    }
    options
}

fn append_suffix(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.as_os_str().to_owned();
    name.push(suffix);
    PathBuf::from(name)
}

struct TemporaryPath(PathBuf);
impl Drop for TemporaryPath {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

fn atomic_replace(path: &Path, bytes: &[u8]) -> Result<(), StoreError> {
    atomic_replace_with(path, bytes, |from, to| fs::rename(from, to))
}

fn atomic_replace_with(
    path: &Path,
    bytes: &[u8],
    commit: impl FnOnce(&Path, &Path) -> std::io::Result<()>,
) -> Result<(), StoreError> {
    static NEXT_TEMP: AtomicU64 = AtomicU64::new(1);
    let mut candidate = None;
    for _ in 0..32 {
        let next = NEXT_TEMP.fetch_add(1, Ordering::Relaxed);
        let temporary = append_suffix(path, &format!(".tmp-{}-{next}", std::process::id()));
        let mut options = private_options();
        options.write(true).create_new(true);
        match options.open(&temporary) {
            Ok(file) => {
                candidate = Some((TemporaryPath(temporary), file));
                break;
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(_) => return Err(StoreError::Io),
        }
    }
    let (temporary, mut file) = candidate.ok_or(StoreError::Io)?;
    file.write_all(bytes).map_err(|_| StoreError::Io)?;
    file.flush().map_err(|_| StoreError::Io)?;
    file.sync_all().map_err(|_| StoreError::Io)?;
    drop(file);
    // Same-directory rename is the commit point. Never delete the old file
    // first, including on Windows: std's rename already supports replacement.
    // Failure before this point leaves the exact old ciphertext untouched.
    commit(&temporary.0, path).map_err(|_| StoreError::Io)
}

#[cfg(test)]
mod tests;
