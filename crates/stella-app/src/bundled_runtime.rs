//! Parameter-free shipping launcher and immutable resource installation.
//!
//! The desktop distribution is one executable. The original path-based asset
//! loaders still read ordinary files after an automatic, verified first launch.
//! Sibling appdata remains stable when the embedded resource content changes.

use std::{
    ffi::OsString,
    fs::{self, File, OpenOptions},
    io::{self, Read, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, Result, bail, ensure};
use flate2::bufread::GzDecoder;
use sha2::{Digest, Sha256};

use crate::bundle_format::{BUNDLE_MAGIC, INSTALLED_MANIFEST, Manifest, copy_hashed, hex_digest};

#[cfg(all(not(debug_assertions), not(test)))]
pub(super) fn run() -> Result<()> {
    let root = user_data_root(std::env::consts::OS, |name| std::env::var_os(name))?;
    let data = install_resources(
        &root,
        include_bytes!(concat!(env!("OUT_DIR"), "/runtime.manifest.json")),
        include_bytes!(concat!(env!("OUT_DIR"), "/runtime.bundle.gz")),
    )?;
    let mut app = crate::StellaApp::new_with_missing_global_diagnostics(
        data,
        crate::GameResolution::default(),
        false,
        crate::PlatformServiceOptions {
            local_services: true,
            ..Default::default()
        },
    )?;
    let event_loop = crate::EventLoop::new().context("create event loop")?;
    let result = event_loop.run_app(&mut app).context("run Stella desktop");
    app.finish_window_run(result)
}

#[cfg(all(target_os = "windows", not(debug_assertions), not(test)))]
pub(super) fn show_error(error: &anyhow::Error) {
    #[link(name = "user32")]
    unsafe extern "system" {
        fn MessageBoxW(
            window: *mut std::ffi::c_void,
            text: *const u16,
            caption: *const u16,
            flags: u32,
        ) -> i32;
    }
    let text: Vec<u16> = format!("Unable to start Angry Birds Stella: Rehost\n\n{error:#}")
        .encode_utf16()
        .chain([0])
        .collect();
    let caption: Vec<u16> = "Angry Birds Stella: Rehost"
        .encode_utf16()
        .chain([0])
        .collect();
    // Both UTF-16 buffers are NUL-terminated and live through the synchronous
    // Win32 dialog. No console exists in the shipping Windows subsystem.
    unsafe {
        MessageBoxW(std::ptr::null_mut(), text.as_ptr(), caption.as_ptr(), 0x10);
    }
}

fn user_data_root(os: &str, lookup: impl Fn(&str) -> Option<OsString>) -> Result<PathBuf> {
    // A non-CLI override permits isolated QA and explicitly chosen portable
    // installations without redirecting the operating system's HOME variable.
    if let Some(path) = lookup("STELLA_USER_DATA_DIR") {
        return absolute_directory(path, "STELLA_USER_DATA_DIR");
    }
    match os {
        "macos" => Ok(
            absolute_directory(lookup("HOME").context("HOME is unset")?, "HOME")?
                .join("Library/Application Support/Angry Birds Stella Rehost"),
        ),
        "windows" => Ok(absolute_directory(
            lookup("LOCALAPPDATA").context("LOCALAPPDATA is unset")?,
            "LOCALAPPDATA",
        )?
        .join("Angry Birds Stella Rehost")),
        "linux" => match lookup("XDG_DATA_HOME") {
            Some(path) if !path.is_empty() => {
                Ok(absolute_directory(path, "XDG_DATA_HOME")?.join("angry-birds-stella-rehost"))
            }
            _ => Ok(
                absolute_directory(lookup("HOME").context("HOME is unset")?, "HOME")?
                    .join(".local/share/angry-birds-stella-rehost"),
            ),
        },
        _ => bail!("unsupported desktop platform: {os}"),
    }
}

fn absolute_directory(value: OsString, name: &str) -> Result<PathBuf> {
    let path = PathBuf::from(value);
    ensure!(path.is_absolute(), "{name} must be an absolute directory");
    Ok(path)
}

fn install_resources(
    user_root: &Path,
    manifest_bytes: &[u8],
    compressed: &[u8],
) -> Result<PathBuf> {
    let compressed = compressed
        .strip_prefix(BUNDLE_MAGIC)
        .context("invalid embedded resource bundle header")?;
    let manifest: Manifest =
        serde_json::from_slice(manifest_bytes).context("read embedded resource manifest")?;
    manifest.validate()?;
    // The manifest includes every file's size and SHA-256. Its content hash is
    // independent of compression settings, timestamps and build-machine paths.
    let id = hex_digest(&Sha256::digest(manifest_bytes));
    let runtime_root = user_root.join("runtime");
    fs::create_dir_all(&runtime_root)
        .with_context(|| format!("create {}", runtime_root.display()))?;
    ensure_directory(&runtime_root)?;
    let installed = runtime_root.join(&id);
    if fs::symlink_metadata(&installed).is_ok() {
        verify_installation(&installed, &manifest, manifest_bytes)?;
        return Ok(installed);
    }

    let staging = StagingDirectory::create(&runtime_root, &id)?;
    let mut decoder = GzDecoder::new(compressed);
    for resource in &manifest.files {
        let path = staging.0.join(&resource.path);
        fs::create_dir_all(path.parent().context("resource parent is missing")?)?;
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .with_context(|| format!("unpack {}", resource.path))?;
        let (size, hash) = copy_hashed(&mut decoder.by_ref().take(resource.size), &mut file)
            .with_context(|| format!("decode {}", resource.path))?;
        ensure!(
            size == resource.size && hash == resource.sha256,
            "embedded resource is incomplete or corrupt: {}",
            resource.path
        );
    }
    let mut extra = [0];
    ensure!(
        decoder.read(&mut extra)? == 0,
        "unexpected trailing resource content"
    );
    ensure!(
        decoder.into_inner().is_empty(),
        "unexpected trailing compressed content"
    );
    let mut marker = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(staging.0.join(INSTALLED_MANIFEST))?;
    marker.write_all(manifest_bytes)?;
    marker.sync_all()?;
    drop(marker);
    match fs::rename(&staging.0, &installed) {
        Ok(()) => {}
        Err(error) => {
            // Another first launch may finish before this one. It must be a
            // complete, identical installation before its resources are used.
            if fs::symlink_metadata(&installed).is_err() {
                return Err(error).context("install embedded resource directory");
            }
            verify_installation(&installed, &manifest, manifest_bytes)?;
        }
    }
    Ok(installed)
}

fn ensure_directory(path: &Path) -> Result<()> {
    ensure!(
        fs::symlink_metadata(path)?.file_type().is_dir(),
        "resource directory is missing or a symlink: {}",
        path.display()
    );
    Ok(())
}

fn verify_installation(root: &Path, manifest: &Manifest, manifest_bytes: &[u8]) -> Result<()> {
    ensure_directory(root)?;
    let marker = root.join(INSTALLED_MANIFEST);
    ensure!(
        fs::symlink_metadata(&marker)?.file_type().is_file(),
        "resource manifest is missing or a symlink: {}",
        marker.display()
    );
    ensure!(
        fs::read(marker)? == manifest_bytes,
        "installed resource manifest changed: {}",
        root.display()
    );
    for resource in &manifest.files {
        let mut path = root.to_path_buf();
        let mut parts = resource.path.split('/').peekable();
        while let Some(part) = parts.next() {
            path.push(part);
            if parts.peek().is_some() {
                ensure_directory(&path)?;
            }
        }
        let metadata = fs::symlink_metadata(&path)
            .with_context(|| format!("installed resource is missing: {}", path.display()))?;
        ensure!(
            metadata.file_type().is_file() && metadata.len() == resource.size,
            "installed resource changed: {}",
            path.display()
        );
        let (size, hash) = copy_hashed(&mut File::open(&path)?, &mut io::sink())?;
        ensure!(
            size == resource.size && hash == resource.sha256,
            "installed resource SHA-256 mismatch: {}",
            path.display()
        );
    }
    Ok(())
}

struct StagingDirectory(PathBuf);

impl StagingDirectory {
    fn create(parent: &Path, id: &str) -> Result<Self> {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let time = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
        let path = parent.join(format!(
            ".unpack-{id}-{}-{time}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).context("create resource staging directory")?;
        Ok(Self(path))
    }
}

impl Drop for StagingDirectory {
    fn drop(&mut self) {
        if let Err(error) = fs::remove_dir_all(&self.0)
            && error.kind() != io::ErrorKind::NotFound
        {
            eprintln!(
                "remove resource staging directory {}: {error}",
                self.0.display()
            );
        }
    }
}

#[cfg(test)]
mod tests;
