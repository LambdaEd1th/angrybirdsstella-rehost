//! Downloadable Assets service and named sprite-sheet lifetime.

use crate::*;
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeSet, VecDeque},
    fs,
    io::Read,
    time::Duration,
};

const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
const CACHE_DIRECTORY: &str = "assets_service";
const CACHE_INDEX: &str = "assets_service/.stella-assets.json";

#[derive(Clone, Debug)]
enum Completion {
    Success(BTreeMap<String, String>),
    Error {
        failed: Vec<String>,
        code: i32,
        message: String,
    },
}

/// Main-thread completion state retained by Purple's native Assets object.
#[derive(Clone, Debug)]
pub(crate) struct AssetsRuntime {
    compatible_url: Arc<Mutex<Option<String>>>,
    downloaded_asset_names: Arc<Mutex<BTreeMap<String, String>>>,
    completions: Arc<Mutex<VecDeque<Completion>>>,
    cache_access: Arc<Mutex<()>>,
    application_events: ApplicationEventScheduler,
}

impl AssetsRuntime {
    fn new(application_events: ApplicationEventScheduler) -> Self {
        Self {
            compatible_url: Arc::new(Mutex::new(None)),
            downloaded_asset_names: Arc::new(Mutex::new(BTreeMap::new())),
            completions: Arc::new(Mutex::new(VecDeque::new())),
            cache_access: Arc::new(Mutex::new(())),
            application_events,
        }
    }

    pub(crate) fn set_compatible_url(&self, url: &str) -> LuaResult<()> {
        let url = url.trim();
        let host = url
            .strip_prefix("http://")
            .or_else(|| url.strip_prefix("https://"));
        let Some(host) = host else {
            return Err(runtime_error(
                "assets URL must use the http or https scheme",
            ));
        };
        if host.is_empty() || host.starts_with('/') {
            return Err(runtime_error("assets URL is missing a host"));
        }
        *self
            .compatible_url
            .lock()
            .map_err(|_| runtime_error("assets URL lock poisoned"))? = Some(url.to_owned());
        Ok(())
    }

    fn compatible_url(&self) -> Option<String> {
        self.compatible_url
            .lock()
            .expect("assets URL lock poisoned")
            .clone()
    }

    fn push(&self, completion: Completion) {
        let mut completions = self
            .completions
            .lock()
            .expect("assets completion queue lock poisoned");
        completions.push_back(completion);
        self.application_events.post(ApplicationEvent::Assets);
    }

    fn pop_pending(&self) -> Option<Completion> {
        self.completions
            .lock()
            .expect("assets completion queue lock poisoned")
            .pop_front()
    }

    pub(crate) fn discard_completion(&self) {
        let _ = self.pop_pending();
    }
}

#[derive(Clone, Debug, Deserialize)]
struct AssetManifest {
    assets: Vec<AssetInfo>,
    #[serde(rename = "failedAssets")]
    failed_assets: Vec<String>,
}

#[derive(Clone, Debug, Deserialize)]
struct AssetInfo {
    name: String,
    #[serde(default, rename = "cdnURL")]
    cdn_url: Option<String>,
    #[serde(default)]
    url: Option<String>,
    hash: String,
    size: u64,
}

impl AssetInfo {
    fn download_url(&self) -> Result<&str, String> {
        // sub_1006F9A34 chooses cdnURL based on field presence, not whether
        // its string is empty. Only an absent cdnURL falls back to url; the
        // subsequent empty-string guard throws the native error below.
        let url = self
            .cdn_url
            .as_deref()
            .or(self.url.as_deref())
            .unwrap_or_default();
        if url.is_empty() {
            Err("Received empty asset URL from server".to_owned())
        } else {
            Ok(url)
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
struct CacheIndex {
    assets: BTreeMap<String, CachedAsset>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct CachedAsset {
    hash: String,
    size: u64,
    filename: String,
}

fn read_cache_index(data_root: &Path) -> CacheIndex {
    app_data_path(data_root, CACHE_INDEX)
        .ok()
        .and_then(|path| fs::read(path).ok())
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

fn write_cache_index(data_root: &Path, index: &CacheIndex) -> Result<(), String> {
    let path = app_data_path(data_root, CACHE_INDEX).map_err(|error| error.to_string())?;
    let parent = path
        .parent()
        .ok_or_else(|| "assets cache path has no parent".to_owned())?;
    fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    let bytes = serde_json::to_vec_pretty(index).map_err(|error| error.to_string())?;
    fs::write(path, bytes).map_err(|error| error.to_string())
}

fn read_http_body(mut response: ureq::http::Response<ureq::Body>) -> Result<Vec<u8>, String> {
    if response.status().as_u16() != 200 {
        return Err(format!("HTTP {}", response.status().as_u16()));
    }
    let mut body = Vec::new();
    response
        .body_mut()
        .as_reader()
        .read_to_end(&mut body)
        .map_err(|error| error.to_string())?;
    Ok(body)
}

fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .http_status_as_error(false)
        .timeout_global(Some(REQUEST_TIMEOUT))
        .build()
        .new_agent()
}

fn request_manifest(url: &str, requested: &[String]) -> Result<AssetManifest, String> {
    // NewAssetRequest (sub_1006F91B8) uses service apdrive/1 and the `assets`
    // route. sub_1006E5A48 adds one query pair named `name` for every entry.
    let mut request = agent().get(url);
    for name in requested {
        request = request.query("name", name);
    }
    let response = request.call().map_err(|error| error.to_string())?;
    let body = read_http_body(response)?;
    serde_json::from_slice(&body).map_err(|error| error.to_string())
}

fn download_asset(url: &str) -> Result<Vec<u8>, String> {
    let host = url
        .strip_prefix("http://")
        .or_else(|| url.strip_prefix("https://"));
    if host.is_none_or(|host| host.is_empty() || host.starts_with('/')) {
        return Err("asset download URL must use http or https".to_owned());
    }
    let response = agent().get(url).call().map_err(|error| error.to_string())?;
    read_http_body(response)
}

fn load_online_assets(
    data_root: &Path,
    url: &str,
    requested: &[String],
) -> Result<BTreeMap<String, String>, Completion> {
    let manifest = request_manifest(url, requested).map_err(|message| Completion::Error {
        failed: requested.to_vec(),
        code: -2,
        message: format!("Unable to load resource: {message}"),
    })?;
    if !manifest.failed_assets.is_empty() {
        // sub_1006F0B00 passes the full requested vector first, followed by
        // failedAssets, -1 and "Assets not found". The Lua bridge ignores the
        // second vector and therefore observes every requested name here.
        return Err(Completion::Error {
            failed: requested.to_vec(),
            code: -1,
            message: "Assets not found".to_owned(),
        });
    }

    let requested = requested.iter().cloned().collect::<BTreeSet<_>>();
    let mut cache_index = read_cache_index(data_root);
    let mut available = BTreeMap::new();
    for asset in manifest.assets {
        if !requested.contains(&asset.name) {
            continue;
        }
        let filename = format!("{CACHE_DIRECTORY}/{}", asset.name);
        let destination =
            app_data_path(data_root, &filename).map_err(|error| Completion::Error {
                failed: requested.iter().cloned().collect(),
                code: -2,
                message: format!("Unable to save file to device: {error}"),
            })?;
        let cached = cache_index.assets.get(&asset.name).is_some_and(|cached| {
            cached.hash == asset.hash
                && cached.size == asset.size
                && cached.filename == filename
                && destination
                    .metadata()
                    .is_ok_and(|metadata| metadata.len() == asset.size)
        });
        if !cached {
            let download_url = asset.download_url().map_err(|message| Completion::Error {
                failed: requested.iter().cloned().collect(),
                code: -2,
                message,
            })?;
            let bytes = download_asset(download_url).map_err(|message| Completion::Error {
                failed: requested.iter().cloned().collect(),
                code: -2,
                message: format!("Unable to load resource {} : {message}", asset.name),
            })?;
            if bytes.len() as u64 != asset.size {
                return Err(Completion::Error {
                    failed: requested.iter().cloned().collect(),
                    code: -2,
                    message: "Incorrect file size".to_owned(),
                });
            }
            let parent = destination.parent().ok_or_else(|| Completion::Error {
                failed: requested.iter().cloned().collect(),
                code: -2,
                message: "Unable to save file to device".to_owned(),
            })?;
            fs::create_dir_all(parent).map_err(|error| Completion::Error {
                failed: requested.iter().cloned().collect(),
                code: -2,
                message: format!("Unable to save file to device: {error}"),
            })?;
            fs::write(&destination, bytes).map_err(|error| Completion::Error {
                failed: requested.iter().cloned().collect(),
                code: -2,
                message: format!("Unable to save file to device: {error}"),
            })?;
        }
        cache_index.assets.insert(
            asset.name.clone(),
            CachedAsset {
                hash: asset.hash,
                size: asset.size,
                filename: filename.clone(),
            },
        );
        available.insert(asset.name, filename);
    }
    write_cache_index(data_root, &cache_index).map_err(|message| Completion::Error {
        failed: requested.iter().cloned().collect(),
        code: -2,
        message: format!("Unable to save file to device: {message}"),
    })?;
    Ok(available)
}

fn enqueue_online_load(
    runtime: &AssetsRuntime,
    data_root: Arc<PathBuf>,
    url: String,
    requested: Vec<String>,
) -> LuaResult<()> {
    let worker_runtime = runtime.clone();
    std::thread::Builder::new()
        .name("stella-assets".to_owned())
        .spawn(move || {
            let _cache_guard = worker_runtime
                .cache_access
                .lock()
                .expect("assets cache lock poisoned");
            let completion = match load_online_assets(&data_root, &url, &requested) {
                Ok(available) => Completion::Success(available),
                Err(completion) => completion,
            };
            worker_runtime.push(completion);
        })
        .map_err(|_| runtime_error("Creating thread failed"))?;
    Ok(())
}

pub(super) fn install(
    lua: &Lua,
    globals: &mlua::Table,
    data_root: Arc<PathBuf>,
    resource_runtime: Arc<Mutex<ResourceRuntime>>,
    application_events: ApplicationEventScheduler,
) -> LuaResult<AssetsRuntime> {
    let runtime = AssetsRuntime::new(application_events);
    let downloadable_assets = lua.create_table()?;
    // Purple's Assets constructor sub_1000AC118 publishes only loadFiles and
    // createSpriteSheet. getAssetFilename/haveBeenDownloaded are defined by
    // scripts_common/cloud/rovioid/Assets.lua after the native table exists.
    let load_asset_root = Arc::clone(&data_root);
    let load_runtime = runtime.clone();
    downloadable_assets.set(
        "loadFiles",
        lua.create_function(move |_, args: MultiValue| {
            // Generated adapter sub_1000AD28C -> sub_1000AD2F4 requires an
            // exact table in slot one and never checks the remaining stack.
            let requested = native_required_table(&args, 0, "Assets.loadFiles")?;
            // Assets::loadFiles (sub_1000AC25C) traverses every table value,
            // starts the asynchronous RCS request and later calls exactly one
            // of onLoadSuccess(table) or onLoadError(array, code, message).
            // The discontinued service is represented by its on-disk cache.
            let mut requested_names = Vec::new();
            for pair in requested.pairs::<Value, Value>() {
                let (_, value) = pair?;
                let name = value_string(&value).ok_or_else(|| {
                    runtime_error("Assets.loadFiles table values must be strings")
                })?;
                requested_names.push(name);
            }

            if let Some(url) = load_runtime.compatible_url() {
                return enqueue_online_load(
                    &load_runtime,
                    Arc::clone(&load_asset_root),
                    url,
                    requested_names,
                );
            }

            let mut available = BTreeMap::new();
            let mut missing = Vec::new();
            let all_requested = requested_names.clone();
            {
                let known = load_runtime
                    .downloaded_asset_names
                    .lock()
                    .expect("downloaded asset map lock poisoned");
                for requested in requested_names {
                    if let Some(filename) = known.get(&requested) {
                        available.insert(requested, filename.clone());
                    } else if app_data_path(&load_asset_root, &requested)
                        .is_ok_and(|path| path.is_file())
                    {
                        available.insert(requested.clone(), requested);
                    } else {
                        let filename = format!("{CACHE_DIRECTORY}/{requested}");
                        if app_data_path(&load_asset_root, &filename)
                            .is_ok_and(|path| path.is_file())
                        {
                            available.insert(requested, filename);
                        } else {
                            missing.push(requested);
                        }
                    }
                }
            }

            if missing.is_empty() {
                load_runtime.push(Completion::Success(available));
            } else {
                load_runtime.push(Completion::Error {
                    failed: all_requested,
                    code: -1,
                    message: "Assets not found".to_owned(),
                });
            }
            Ok(())
        })?,
    )?;
    let downloadable_sheet_resources = Arc::clone(&resource_runtime);
    let downloadable_sheet_root = Arc::clone(&data_root);
    downloadable_assets.set(
        "createSpriteSheet",
        lua.create_function(move |_, args: MultiValue| {
            // Generated adapter sub_1000ACDE8 delegates to sub_1000ACE50,
            // which reads all three slots through the exact STRING-tag
            // accessor sub_1005285CC and ignores later stack values.
            let name = native_required_string(&args, 0, "Assets.createSpriteSheet")?;
            let descriptor = native_required_string(&args, 1, "Assets.createSpriteSheet")?;
            let texture = native_required_string(&args, 2, "Assets.createSpriteSheet")?;
            // sub_1000AC660 constructs the sheet from the descriptor and
            // texture paths completely before sub_100457724 transactionally
            // replaces the shared Resources map entry under `name`.
            let descriptor_path = app_data_path(&downloadable_sheet_root, &descriptor)
                .ok()
                .filter(|path| path.is_file())
                .or_else(|| resolve_data_file(&downloadable_sheet_root, &descriptor).ok())
                .ok_or_else(|| runtime_error(format!("asset was not found: {descriptor}")))?;
            let mut sheet = load_sprite_sheet_path(&descriptor_path, &descriptor)?;
            sheet.textures.clear();
            sheet.textures.push(texture);
            sheet.sprite_texture_indices.fill(0);
            let mut resources = downloadable_sheet_resources
                .lock()
                .expect("resource runtime lock poisoned");
            resources.replace_sprite_sheet_value(&name, sheet);
            resources.sprite_sheets.insert(name.clone());
            resources
                .sprite_sheet_paths
                .insert(name.clone(), descriptor);
            resources
                .sprite_sheet_descriptor_paths
                .insert(name.clone(), descriptor_path);
            resources.cache_sprite_sheet_host_bindings(&name, &downloadable_sheet_root);
            Ok(())
        })?,
    )?;
    globals.set("Assets", downloadable_assets)?;
    Ok(runtime)
}

/// Deliver RCS Assets request functors on the application thread.
pub(crate) fn dispatch_completion(lua: &Lua, runtime: &AssetsRuntime) -> LuaResult<()> {
    // The completion functors at sub_1000AC964/sub_1000ACA0C retain the
    // native Assets LuaObject created by sub_1000AC118. The shipped facade is
    // a separate GameLua-environment table and installs its callbacks onto
    // this root object through `_G.Assets`.
    let native_assets = lua.globals().get::<mlua::Table>("Assets")?;
    let Some(completion) = runtime.pop_pending() else {
        return Ok(());
    };
    match completion {
        Completion::Success(available) => {
            runtime
                .downloaded_asset_names
                .lock()
                .expect("downloaded asset map lock poisoned")
                .extend(available.clone());
            let result = lua.create_table()?;
            for (requested, filename) in available {
                result.raw_set(requested, filename)?;
            }
            native_assets
                .get::<mlua::Function>("onLoadSuccess")?
                .call::<()>(result)?;
        }
        Completion::Error {
            failed,
            code,
            message,
        } => {
            let failed_table = lua.create_table()?;
            for (index, filename) in failed.into_iter().enumerate() {
                failed_table.raw_set(index + 1, filename)?;
            }
            native_assets
                .get::<mlua::Function>("onLoadError")?
                .call::<()>((failed_table, code, message))?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn present_empty_cdn_url_does_not_fall_back_to_legacy_url() {
        let asset: AssetInfo = serde_json::from_value(serde_json::json!({
            "name": "dynamic.bin",
            "cdnURL": "",
            "url": "https://legacy.invalid/dynamic.bin",
            "hash": "hash-1",
            "size": 1
        }))
        .unwrap();

        assert_eq!(
            asset.download_url().unwrap_err(),
            "Received empty asset URL from server"
        );
    }
}
