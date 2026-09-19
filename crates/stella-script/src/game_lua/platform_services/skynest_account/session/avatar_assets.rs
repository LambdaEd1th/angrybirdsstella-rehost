//! Native personal.imageAssets and IdentityLevel2::fetchAvatarAssets.

use super::*;
use crate::SdkLogLevel;
use serde_json::Value;
use std::{
    fs,
    io::{Read, Write},
    path::Path,
};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(in super::super) struct AvatarAsset {
    pub(in super::super) avatar_id: String,
    pub(in super::super) url: String,
    pub(in super::super) hash: String,
    // 100676E20 does not initialize these stack slots before the first
    // Number assignment. Preserve inheritance, but never invent that initial
    // indeterminate value or let it control filesystem/network operations.
    pub(in super::super) size: Option<i64>,
    pub(in super::super) dimension: Option<i32>,
}

pub(in super::super) fn parse_avatar_assets(value: &Value) -> Vec<AvatarAsset> {
    let Some(personal) = value.get("personal").and_then(Value::as_object) else {
        return Vec::new();
    };
    let Some(values) = personal.get("imageAssets").and_then(Value::as_array) else {
        return Vec::new();
    };
    let mut current = AvatarAsset {
        avatar_id: personal
            .get("avatarId")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned(),
        ..AvatarAsset::default()
    };
    values
        .iter()
        .map(|value| {
            // 100677398 gates BOTH assignments on typed url AND typed hash.
            // The record lives outside the loop, so absent fields retain prior
            // values. Every array entry is appended, including malformed entries.
            if let (Some(url), Some(hash)) = (
                value.get("url").and_then(Value::as_str),
                value.get("hash").and_then(Value::as_str),
            ) {
                current.url = url.to_owned();
                current.hash = hash.to_owned();
            }
            if let Some(value) = value.get("dimension").filter(|v| v.is_number()) {
                current.dimension = protocol::signed_number(value).ok().map(|n| n as i32);
            }
            if let Some(value) = value.get("size").filter(|v| v.is_number()) {
                current.size = protocol::signed_number(value).ok();
            }
            current.clone()
        })
        .collect()
}

impl IdentitySession {
    fn avatar_operation<T>(
        &self,
        owner: OwnProfileOwner,
        active: Option<&Mutex<bool>>,
        operation: impl FnOnce(&mut ProfileResponse, &dyn RefreshStore) -> Result<T, String>,
    ) -> Result<T, String> {
        // Platform cancellation takes this permit before the identity lock,
        // matching profile publication's lock order. Retiring a provider waits
        // for the current mutation and prevents every subsequent file write.
        let permit = active
            .map(|active| active.lock().map_err(|_| "platform lease lock poisoned"))
            .transpose()?;
        if permit.as_ref().is_some_and(|active| !**active) {
            return Err("platform social request was cancelled".to_owned());
        }
        let mut state = self.state.lock().expect("identity session lock poisoned");
        state
            .check_owner(owner.epoch, Some(owner.generation))
            .map_err(|e| e.to_string())?;
        let profile = state
            .profile
            .as_mut()
            .ok_or_else(|| "identity request cancelled".to_owned())?;
        operation(profile, self.store().as_ref())
    }

    pub(in super::super) fn fetch_avatar_assets(
        &self,
        owner: OwnProfileOwner,
        assets: &[AvatarAsset],
    ) -> Result<(), String> {
        self.fetch_avatar_assets_inner(owner, assets, None)
    }

    pub(in super::super) fn fetch_platform_avatar_assets(
        &self,
        owner: OwnProfileOwner,
        assets: &[AvatarAsset],
        active: &Mutex<bool>,
    ) -> Result<(), String> {
        self.fetch_avatar_assets_inner(owner, assets, Some(active))
    }

    fn fetch_avatar_assets_inner(
        &self,
        owner: OwnProfileOwner,
        assets: &[AvatarAsset],
        active: Option<&Mutex<bool>>,
    ) -> Result<(), String> {
        for asset in assets {
            let dimension = asset
                .dimension
                .ok_or("avatar dimension is indeterminate in native input")?;
            let expected_size = asset
                .size
                .ok_or("avatar size is indeterminate in native input")?;
            // Native takes the substring after the LAST slash, without URL
            // decoding or removing its query string. No slash yields empty.
            let basename = asset.url.rsplit_once('/').map_or("", |(_, name)| name);
            validate_basename(basename)?;
            let native_path = format!("avatarAssets/{basename}");
            let (directory, version) = self.avatar_operation(owner, active, |_, store| {
                let directory = store
                    .avatar_directory()
                    .ok_or("identity avatar cache is unavailable")?;
                let version = store
                    .load_avatar_version(basename)
                    .map_err(|e| e.to_string())?;
                Ok((directory, version))
            })?;
            let path = directory.join(basename);
            if !version.is_empty() && version == asset.hash {
                let cached_size = match open_cached(&path) {
                    Ok(file) => {
                        // Map assignment precedes the size test and survives
                        // a failed replacement or a failed size query.
                        self.avatar_operation(owner, active, |profile, _| {
                            profile.avatar_paths.insert(dimension, native_path.clone());
                            Ok(())
                        })?;
                        file.metadata().map(|metadata| metadata.len() as i32 as i64)
                    }
                    Err(error) => Err(error),
                };
                match cached_size {
                    Ok(actual) if actual == expected_size => continue,
                    Ok(_) => {}
                    Err(error) => {
                        self.avatar_operation(owner, active, |_, _| Ok(()))?;
                        let detail = error.to_string();
                        let suffix = error
                            .raw_os_error()
                            .map(|code| format!(" (os error {code})"));
                        let native_detail = suffix
                            .as_deref()
                            .and_then(|suffix| detail.strip_suffix(suffix))
                            .unwrap_or(&detail);
                        // 100673064 uses the empty tag and errno text.
                        self.sdk_logger.submit(self, SdkLogLevel::Warn, "", &format!(
                            "Unable to open local file while it was supposed to exist: Failed to open file {native_path} : {native_detail} "
                        ));
                        eprintln!("avatar cache file could not be opened: {native_path}: {error}");
                    }
                }
            }
            // Native opens/truncates the final file BEFORE HTTP, with parent
            // creation enabled. Failure leaves partial bytes, not an atomic
            // replacement masquerading as the original behavior.
            let mut file = self.avatar_operation(owner, active, |_, _| {
                create_directory(&directory)?;
                let mut options = registry_store::private_options();
                options.write(true).create(true).truncate(true);
                options
                    .open(&path)
                    .map_err(|e| format!("avatar cache write failed: {e}"))
            })?;
            if !asset.url.starts_with("http://") && !asset.url.starts_with("https://") {
                return Err("avatar asset URL must use http or https".to_owned());
            }
            // No identity access token, segment, client key or credential body
            // belongs on this plain resource GET. Keep the existing explicit
            // URL/no-redirect host transport boundary.
            let mut response = super::super::agent()
                .get(&asset.url)
                .call()
                .map_err(|_| "avatar asset transport error".to_owned())?;
            let status = response.status().as_u16();
            let mut reader = response.body_mut().as_reader();
            let mut bytes = 0i64;
            let mut buffer = [0u8; 16 * 1024];
            loop {
                let count = reader
                    .read(&mut buffer)
                    .map_err(|_| "avatar asset body read failed")?;
                if count == 0 {
                    break;
                }
                bytes += count as i64;
                // Explicit host disk-resource ceiling, not native metadata.
                if bytes > 64 * 1024 * 1024 {
                    return Err("avatar asset exceeds host transfer limit".to_owned());
                }
                self.avatar_operation(owner, active, |_, _| {
                    file.write_all(&buffer[..count])
                        .map_err(|e| format!("avatar cache write failed: {e}"))
                })?;
            }
            self.avatar_operation(owner, active, |_, _| {
                file.flush()
                    .map_err(|e| format!("avatar cache write failed: {e}"))
            })?;
            // Read/write the error response body too, matching the HTTP writer
            // callback that runs before the native exact-200 check.
            if status != 200 {
                return Err(format!("avatar asset HTTP status {status}"));
            }
            if bytes != expected_size {
                return Err("Incorrect filesize".to_owned());
            }
            self.avatar_operation(owner, active, |profile, store| {
                store
                    .store_avatar_version(basename, &asset.hash)
                    .map_err(|e| e.to_string())?;
                profile.avatar_paths.insert(dimension, native_path);
                Ok(())
            })?;
        }
        Ok(())
    }
}

fn validate_basename(name: &str) -> Result<(), String> {
    if name.is_empty() || matches!(name, "." | "..") || name.contains(['\\', '\0']) {
        return Err("avatar cache filename is unsafe".to_owned());
    }
    Ok(())
}

fn create_directory(directory: &Path) -> Result<(), String> {
    match fs::symlink_metadata(directory) {
        Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => Ok(()),
        Ok(_) => Err("avatar cache directory is not a directory".to_owned()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => fs::create_dir_all(directory)
            .map_err(|e| format!("avatar cache directory creation failed: {e}")),
        Err(error) => Err(format!("avatar cache directory read failed: {error}")),
    }
}

fn open_cached(path: &Path) -> std::io::Result<fs::File> {
    if path.parent().is_some_and(|parent| {
        fs::symlink_metadata(parent).is_ok_and(|metadata| metadata.file_type().is_symlink())
    }) {
        return Err(std::io::Error::other(
            "avatar cache directory is a symbolic link",
        ));
    }
    let mut options = registry_store::private_options();
    options.read(true);
    let file = options.open(path)?;
    if !file.metadata()?.is_file() {
        return Err(std::io::Error::other(
            "avatar cache entry is not a regular file",
        ));
    }
    Ok(file)
}

#[cfg(test)]
mod tests;
