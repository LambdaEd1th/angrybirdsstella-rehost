//! Native UserProfileRequest / ContentCache disk and serialized worker path.

use super::super::skynest_account::avatar_support::{AvatarCacheRegistry, SdkLogSink};
use crate::{
    SdkLogLevel,
    game_lua::platform::{sha1_digest, upper_hex},
};
use std::{
    collections::BTreeSet,
    fs::{self, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::{Arc, Mutex, mpsc},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

const DIRECTORY: &str = "SkynestUserAvatars";
const LIMIT: u64 = 5 * 1024 * 1024;
const HOST_TRANSFER_LIMIT: u64 = 64 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum CacheError {
    Cancelled,
    Failed(String),
}
impl From<String> for CacheError {
    fn from(value: String) -> Self {
        Self::Failed(value)
    }
}
impl From<std::io::Error> for CacheError {
    fn from(value: std::io::Error) -> Self {
        Self::Failed(value.to_string())
    }
}

type Done = Box<dyn FnOnce(Result<PathBuf, CacheError>) + Send>;
enum Job {
    Download { url: String, done: Done },
    Touch(PathBuf),
}

#[derive(Clone)]
pub(super) struct AvatarCache {
    backend: Arc<Backend>,
    sender: mpsc::Sender<Job>,
}

struct Backend {
    directory: PathBuf,
    registry: AvatarCacheRegistry,
    active: Mutex<bool>,
    retained: Mutex<BTreeSet<PathBuf>>,
    log: SdkLogSink,
}

impl std::fmt::Debug for AvatarCache {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("AvatarCache")
    }
}

impl AvatarCache {
    pub(super) fn new(root: PathBuf, log: SdkLogSink) -> Result<Self, String> {
        let backend = Arc::new(Backend {
            registry: AvatarCacheRegistry::open(root.join("fusion.registry"))?,
            directory: root.join(DIRECTORY),
            active: Mutex::new(true),
            retained: Mutex::new(BTreeSet::new()),
            log,
        });
        let (sender, receiver) = mpsc::channel::<Job>();
        let worker = backend.clone();
        std::thread::Builder::new()
            .name("stella-avatar-cache".to_owned())
            .spawn(move || {
                while let Ok(job) = receiver.recv() {
                    match job {
                        Job::Download { url, done } => done(worker.fetch(&url)),
                        Job::Touch(path) => {
                            if let Err(CacheError::Failed(detail)) = worker.owned(|| {
                                let file = private_options()
                                    .write(true)
                                    .create(true)
                                    .truncate(false)
                                    .open(path)?;
                                let now = SystemTime::now();
                                file.set_times(
                                    fs::FileTimes::new().set_accessed(now).set_modified(now),
                                )?;
                                Ok(())
                            }) {
                                worker.log.submit(
                                    SdkLogLevel::Warn,
                                    "TaskDispatcher",
                                    &format!("Got exception: {detail}"),
                                );
                                eprintln!("avatar cache touch failed: {detail}");
                            }
                        }
                    }
                }
            })
            .map_err(|_| "Creating thread failed".to_owned())?;
        Ok(Self { backend, sender })
    }

    pub(super) fn retire(&self) {
        *self
            .backend
            .active
            .lock()
            .expect("avatar owner lock poisoned") = false;
    }

    pub(super) fn request(
        &self,
        url: String,
        done: impl FnOnce(Result<PathBuf, CacheError>) + Send + 'static,
    ) -> Result<(), String> {
        // 10068CA18 validates before scheduling the first request for a URL.
        if let Err(error) = self.backend.prepare(epoch_seconds(SystemTime::now())) {
            match error {
                CacheError::Cancelled => return Err("avatar request cancelled".to_owned()),
                CacheError::Failed(detail) => {
                    self.backend.log.submit(
                        SdkLogLevel::Error,
                        "UserProfileRequest/validateAvatarAssetsCache",
                        &format!("Directory check or creation failed : {detail}"),
                    );
                    eprintln!("avatar directory validation failed: {detail}");
                }
            }
        }
        self.sender
            .send(Job::Download {
                url,
                done: Box::new(done),
            })
            .map_err(|_| "avatar cache worker stopped".to_owned())
    }

    pub(super) fn touch(&self, path: PathBuf) -> Result<(), String> {
        self.sender
            .send(Job::Touch(path))
            .map_err(|_| "avatar cache worker stopped".to_owned())
    }
}

impl Backend {
    fn owned<T>(&self, work: impl FnOnce() -> Result<T, CacheError>) -> Result<T, CacheError> {
        let active = self.active.lock().expect("avatar owner lock poisoned");
        if !*active {
            return Err(CacheError::Cancelled);
        }
        work()
    }

    fn prepare(&self, now: i64) -> Result<(), CacheError> {
        self.owned(|| {
            let exists = match fs::symlink_metadata(&self.directory) {
                Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => true,
                Ok(_) => {
                    return Err(CacheError::Failed(
                        "avatar cache directory is not a directory".to_owned(),
                    ));
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
                Err(error) => return Err(error.into()),
            };
            let expired =
                exists && now.wrapping_sub(self.registry.creation_time(DIRECTORY)?) / 86400 >= 7;
            if expired {
                fs::remove_dir_all(&self.directory)?;
            }
            if !exists || expired {
                fs::create_dir(&self.directory)?;
                self.registry.set_creation_time(DIRECTORY, now)?;
            }
            Ok(())
        })
    }

    fn fetch(&self, url: &str) -> Result<PathBuf, CacheError> {
        let basename = cache_basename(url);
        let path = self.directory.join(&basename);
        let temporary = self.directory.join(format!("{basename}.tmp"));
        let result = self.download(url, &path, &temporary);
        if let Err(CacheError::Failed(detail)) = &result {
            // 1006679A4 catches std::exception, ERROR then best-effort removal
            // of BOTH paths before posting (URL,false). IOException removal
            // failures are explicitly caught at100667C18/100667CEC.
            self.owned(|| {
                self.log.submit(
                    SdkLogLevel::Error,
                    "Ads/ContentCache",
                    &format!("Download failed: {detail}"),
                );
                eprintln!("avatar download failed: {detail}");
                for file in [&path, &temporary] {
                    if let Err(error) = fs::remove_file(file)
                        && error.kind() != std::io::ErrorKind::NotFound
                    {
                        eprintln!("avatar failed-file cleanup failed: {error}");
                    }
                }
                Ok(())
            })?;
        }
        result
    }

    fn download(&self, url: &str, path: &Path, temporary: &Path) -> Result<PathBuf, CacheError> {
        let exists = self.owned(|| {
            // validateAvatarAssetsCache catches its own errors; ContentCache
            // still attempts its normal directory/read/download operations.
            if !self.directory.exists() {
                fs::create_dir(&self.directory)?;
            }
            ensure_directory(&self.directory)?;
            match fs::symlink_metadata(path) {
                Ok(meta) if meta.is_file() && !meta.file_type().is_symlink() => Ok(true),
                Ok(_) => Err(CacheError::Failed(
                    "avatar cache entry is not a regular file".to_owned(),
                )),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
                Err(e) => Err(e.into()),
            }
        })?;
        if !exists {
            let mut file = self.owned(|| {
                evict(
                    &self.directory,
                    &self.retained.lock().expect("avatar cache lock poisoned"),
                    LIMIT,
                )?;
                Ok(private_options()
                    .write(true)
                    .create(true)
                    .truncate(true)
                    .open(temporary)?)
            })?;
            if !(url.starts_with("http://") || url.starts_with("https://")) {
                return Err(CacheError::Failed(
                    "avatar URL must use http or https".to_owned(),
                ));
            }
            let mut response = ureq::Agent::config_builder()
                .http_status_as_error(false)
                .max_redirects(0)
                .timeout_global(Some(Duration::from_millis(120000)))
                .build()
                .new_agent()
                .get(url)
                .call()
                .map_err(|_| CacheError::Failed("avatar transport error".to_owned()))?;
            let status = response.status();
            let mut reader = response.body_mut().as_reader();
            let mut buffer = [0u8; 16 * 1024];
            let mut size = 0u64;
            loop {
                let count = reader
                    .read(&mut buffer)
                    .map_err(|_| CacheError::Failed("avatar response read failed".to_owned()))?;
                if count == 0 {
                    break;
                }
                size += count as u64;
                if size > HOST_TRANSFER_LIMIT {
                    return Err(CacheError::Failed(
                        "avatar exceeds host transfer limit".to_owned(),
                    ));
                }
                self.owned(|| {
                    file.write_all(&buffer[..count])?;
                    Ok(())
                })?;
            }
            self.owned(|| {
                file.flush()?;
                Ok(())
            })?;
            if status.as_u16() != 200 {
                return Err(CacheError::Failed(
                    status
                        .canonical_reason()
                        .unwrap_or("Unknown HTTP status")
                        .to_owned(),
                ));
            }
            if size == 0 {
                return Err(CacheError::Failed("Empty response".to_owned()));
            }
            drop(file);
            self.owned(|| {
                fs::rename(temporary, path)?;
                Ok(())
            })?;
        }
        self.owned(|| {
            let file = private_options().read(true).open(path)?;
            if !file.metadata()?.is_file() {
                return Err(CacheError::Failed(
                    "avatar cache entry is not a regular file".to_owned(),
                ));
            }
            self.retained
                .lock()
                .expect("avatar cache lock poisoned")
                .insert(path.to_owned());
            Ok(path.to_owned())
        })
    }
}

fn ensure_directory(path: &Path) -> Result<(), CacheError> {
    let meta = fs::symlink_metadata(path)?;
    if !meta.is_dir() || meta.file_type().is_symlink() {
        return Err(CacheError::Failed(
            "avatar cache directory is not a directory".to_owned(),
        ));
    }
    Ok(())
}

fn private_options() -> OpenOptions {
    let mut options = OpenOptions::new();
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600).custom_flags(libc::O_NOFOLLOW);
    }
    options
}

pub(super) fn cache_basename(url: &str) -> String {
    let mut name = upper_hex(&sha1_digest(url.as_bytes()));
    if let Some(dot) = url.rfind('.')
        && url.len() - dot <= 5
    {
        let suffix = &url[dot + 1..];
        if !suffix.is_empty() {
            name.push('.');
            name.push_str(suffix);
        }
    }
    name
}

fn epoch_seconds(time: SystemTime) -> i64 {
    match time.duration_since(UNIX_EPOCH) {
        Ok(d) => d.as_secs() as i64,
        Err(e) => -(e.duration().as_secs() as i64),
    }
}

fn evict(directory: &Path, retained: &BTreeSet<PathBuf>, limit: u64) -> Result<(), CacheError> {
    let mut entries = Vec::new();
    let mut total = 0u64;
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        let metadata = fs::symlink_metadata(entry.path())?;
        if !metadata.is_file() || metadata.file_type().is_symlink() {
            continue;
        }
        let size = metadata.len();
        total = total.saturating_add(size);
        entries.push((epoch_seconds(metadata.modified()?), entry.path(), size));
    }
    if total <= limit {
        return Ok(());
    }
    // Native std::sort compares only second-resolution mtime, without a
    // filename tie-break or reserving room for the incoming response.
    entries.sort_unstable_by_key(|entry| entry.0);
    for (_, path, size) in entries {
        if total <= limit {
            break;
        }
        if !retained.contains(&path) {
            fs::remove_file(path)?;
            total = total.saturating_sub(size);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;
