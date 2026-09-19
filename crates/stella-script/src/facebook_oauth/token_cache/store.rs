//! Host-owned NSUserDefaults-equivalent domain. No implicit account or path.
use super::property_list::{self, Dictionary, Value};
use super::{CachedToken, FacebookTokenCacheError, now};
use std::{
    fmt,
    fs::{self, File, OpenOptions},
    io::{self, Read, Write},
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
};

const DEFAULT_KEY: &str = "FBAccessTokenInformationKey";

/// A cache shared by sessions belonging to one application preferences domain.
/// `open` reads only the explicitly supplied file; `memory` never touches disk.
/// Clone this handle when replacing a provider in the same application.
#[derive(Clone)]
pub struct FacebookTokenCache {
    state: Arc<Mutex<CacheState>>,
}

struct CacheState {
    preferences: Dictionary,
    path: Option<PathBuf>,
    key: String,
    error: Option<FacebookTokenCacheError>,
}

impl fmt::Debug for FacebookTokenCache {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("FacebookTokenCache")
    }
}

impl Default for FacebookTokenCache {
    fn default() -> Self {
        Self::memory()
    }
}

impl FacebookTokenCache {
    pub fn memory() -> Self {
        Self::from_parts(Dictionary::new(), None, DEFAULT_KEY.into())
    }

    /// Load a binary or XML plist at a host-selected path. A missing file starts
    /// empty and is created on the first cache change. Other read/parse errors
    /// are returned without replacing the file or starting authorization.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, FacebookTokenCacheError> {
        Self::open_with_key(path, DEFAULT_KEY)
    }

    /// Equivalent to initWithUserDefaultTokenInformationKeyName30C990. Other
    /// keys in this preferences domain are preserved when this key changes.
    pub fn open_with_key(
        path: impl AsRef<Path>,
        key: impl Into<String>,
    ) -> Result<Self, FacebookTokenCacheError> {
        let path = path.as_ref().to_owned();
        let preferences = read_preferences(&path)?;
        Ok(Self::from_parts(preferences, Some(path), key.into()))
    }

    fn from_parts(preferences: Dictionary, path: Option<PathBuf>, key: String) -> Self {
        Self {
            state: Arc::new(Mutex::new(CacheState {
                preferences,
                path,
                key,
                error: None,
            })),
        }
    }

    /// Retrieve the most recent unobserved cache error. Native synchronize's
    /// return value does not change authentication success30CA78/30CB38;
    /// failed persistence remains observable here while memory is updated.
    pub fn take_error(&self) -> Option<FacebookTokenCacheError> {
        self.state
            .lock()
            .expect("Facebook token cache lock poisoned")
            .error
            .take()
    }

    pub(in crate::facebook_oauth) fn admitted(
        &self,
        permissions: &[String],
    ) -> Result<Option<CachedToken>, FacebookTokenCacheError> {
        let mut state = self
            .state
            .lock()
            .expect("Facebook token cache lock poisoned");
        let Some(value) = state.preferences.get(&state.key) else {
            return Ok(None);
        };
        let now = now();
        let token = match CachedToken::from_dictionary(value, now) {
            Ok(token) => token,
            Err(error) => {
                state.error = Some(error);
                return Err(error);
            }
        };
        match token {
            Some(token) if token.admits(permissions, now) => Ok(Some(token)),
            Some(_) => {
                state.change(None);
                Ok(None)
            }
            // Invalid structure or whitespace-only token returns nil and is
            // not cleared by the native session constructor318D84.
            None => Ok(None),
        }
    }

    pub(in crate::facebook_oauth) fn cache(&self, token: &CachedToken) {
        let mut state = self
            .state
            .lock()
            .expect("Facebook token cache lock poisoned");
        state.change(Some(token.dictionary()));
    }

    pub(in crate::facebook_oauth) fn clear(&self) {
        self.state
            .lock()
            .expect("Facebook token cache lock poisoned")
            .change(None);
    }
}

impl CacheState {
    fn change(&mut self, value: Option<Value>) {
        match value {
            Some(value) => {
                self.preferences.insert(self.key.clone(), value);
            }
            None => {
                self.preferences.remove(&self.key);
            }
        }
        if let Some(path) = &self.path
            && let Err(error) = persist(path, &self.key, self.preferences.get(&self.key))
        {
            self.error = Some(error);
        }
    }
}

fn read_preferences(path: &Path) -> Result<Dictionary, FacebookTokenCacheError> {
    let mut file = match File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Dictionary::new()),
        Err(error) => return Err(FacebookTokenCacheError::Io(error.kind())),
    };
    if !file
        .metadata()
        .map_err(|e| FacebookTokenCacheError::Io(e.kind()))?
        .is_file()
    {
        return Err(FacebookTokenCacheError::InvalidPreferences);
    }
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)
        .map_err(|e| FacebookTokenCacheError::Io(e.kind()))?;
    property_list::read(&bytes)?
        .into_dictionary()
        .ok_or(FacebookTokenCacheError::InvalidPreferences)
}

struct Temporary(PathBuf);
impl Drop for Temporary {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

fn persist(path: &Path, key: &str, value: Option<&Value>) -> Result<(), FacebookTokenCacheError> {
    static NEXT: AtomicU64 = AtomicU64::new(1);
    static WRITES: Mutex<()> = Mutex::new(());
    let _guard = WRITES
        .lock()
        .expect("Facebook preferences write lock poisoned");
    // Commit only this preference key. Independent cache keys and unrelated
    // application preferences may have changed since this handle was opened.
    let mut preferences = read_preferences(path)?;
    match value {
        Some(value) => {
            preferences.insert(key.to_owned(), value.clone());
        }
        None => {
            preferences.remove(key);
        }
    }
    let bytes = property_list::write(&Value::Dictionary(preferences))?;
    // A private sibling and same-directory rename preserve the previous file
    // on failures. The caller owns and creates the parent application domain.
    let mut created = None;
    for _ in 0..32 {
        let mut name = path.as_os_str().to_owned();
        name.push(format!(
            ".tmp-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let temporary = PathBuf::from(name);
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        match options.open(&temporary) {
            Ok(file) => {
                created = Some((Temporary(temporary), file));
                break;
            }
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(FacebookTokenCacheError::Io(error.kind())),
        }
    }
    let (temporary, mut file) =
        created.ok_or(FacebookTokenCacheError::Io(io::ErrorKind::AlreadyExists))?;
    file.write_all(&bytes)
        .and_then(|()| file.sync_all())
        .map_err(|e| FacebookTokenCacheError::Io(e.kind()))?;
    drop(file);
    fs::rename(&temporary.0, path).map_err(|e| FacebookTokenCacheError::Io(e.kind()))
}
