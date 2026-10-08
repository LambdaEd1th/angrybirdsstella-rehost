//! Release builds contain all playable resources; developer builds need no bundle.

#[path = "src/bundle_format.rs"]
mod bundle_format;

use std::{
    env, fs,
    io::{BufWriter, Write},
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, ensure};
use bundle_format::{
    BUNDLE_MAGIC, FORMAT_VERSION, Manifest, ResourceFile, copy_hashed, hex_digest,
    validate_relative_path,
};
use flate2::{Compression, GzBuilder};
use serde::Deserialize;

fn main() {
    if let Err(error) = build() {
        panic!("cannot embed playable desktop resources: {error:#}");
    }
}

fn build() -> Result<()> {
    println!("cargo:rerun-if-env-changed=STELLA_RUNTIME_DATA");
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=src/bundle_format.rs");
    println!("cargo:rerun-if-changed=../../.github/scripts/runtime-native-assets.json");
    // This is the target's cfg, not the build script's own build-override profile.
    if env::var_os("CARGO_CFG_DEBUG_ASSERTIONS").is_some() {
        return Ok(());
    }
    let crate_root =
        PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").context("missing crate root")?);
    let data = env::var_os("STELLA_RUNTIME_DATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| crate_root.join("../../runtime/data"));
    ensure!(
        data.is_dir(),
        "missing {} (extract and stage Purple.app resources, or set STELLA_RUNTIME_DATA)",
        data.display()
    );
    validate_native_resources(&data)?;
    let mut paths = Vec::new();
    collect_files(&data, &data, &mut paths)?;
    paths.sort();
    let output = PathBuf::from(env::var_os("OUT_DIR").context("missing build output")?);
    let mut bundle = BufWriter::new(fs::File::create(output.join("runtime.bundle.gz"))?);
    bundle.write_all(BUNDLE_MAGIC)?;
    let mut compressed = GzBuilder::new()
        .mtime(0)
        .write(bundle, Compression::default());
    let mut manifest = Manifest {
        version: FORMAT_VERSION,
        files: Vec::with_capacity(paths.len()),
    };
    for path in paths {
        let mut input = fs::File::open(data.join(&path))?;
        let (size, sha256) = copy_hashed(&mut input, &mut compressed)?;
        manifest.files.push(ResourceFile { path, size, sha256 });
    }
    manifest.validate()?;
    compressed
        .finish()?
        .into_inner()
        .map_err(|error| error.into_error())?;
    fs::write(
        output.join("runtime.manifest.json"),
        serde_json::to_vec(&manifest)?,
    )?;
    Ok(())
}

fn collect_files(root: &Path, directory: &Path, paths: &mut Vec<String>) -> Result<()> {
    // Watching each directory also detects additions and removals, including
    // an updated tree selected through the build-time path override.
    println!("cargo:rerun-if-changed={}", directory.display());
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        let name = entry.file_name();
        let name = name.to_str().context("resource filename is not UTF-8")?;
        if matches!(name, ".DS_Store" | "__MACOSX") || name.starts_with("._") {
            continue;
        }
        let kind = entry.file_type()?;
        ensure!(
            !kind.is_symlink(),
            "resource symlinks are not embedded: {}",
            entry.path().display()
        );
        if kind.is_dir() {
            collect_files(root, &entry.path(), paths)?;
        } else {
            ensure!(
                kind.is_file(),
                "unsupported resource file: {}",
                entry.path().display()
            );
            let path = entry
                .path()
                .strip_prefix(root)?
                .components()
                .map(|part| part.as_os_str().to_str().context("non-UTF-8 resource path"))
                .collect::<Result<Vec<_>>>()?
                .join("/");
            validate_relative_path(&path)?;
            paths.push(path);
        }
    }
    Ok(())
}

#[derive(Deserialize)]
struct NativeAssets {
    files: Vec<NativeAsset>,
}

#[derive(Deserialize)]
struct NativeAsset {
    destination: String,
    sha256: String,
}

fn validate_native_resources(data: &Path) -> Result<()> {
    for directory in [
        "animations",
        "audio",
        "config",
        "fonts",
        "images",
        "levels",
        "localization",
        "scripts",
        "scripts_common",
        "shaders",
        "skynestdata",
    ] {
        ensure!(
            data.join(directory).is_dir(),
            "required resource directory is missing: {directory}"
        );
    }
    ensure!(
        data.join("scripts/game.lua").is_file(),
        "scripts/game.lua is missing"
    );
    ensure!(
        data.join("channel_push_notification.wav").is_file(),
        "native notification sound is missing"
    );
    let assets: NativeAssets = serde_json::from_str(include_str!(
        "../../.github/scripts/runtime-native-assets.json"
    ))?;
    for asset in assets.files {
        validate_relative_path(&asset.destination)?;
        let path = data.join(&asset.destination);
        let mut file = fs::File::open(&path)
            .with_context(|| format!("missing native asset {}", path.display()))?;
        let (_, hash) = copy_hashed(&mut file, &mut std::io::sink())?;
        ensure!(
            hex_digest(&hash) == asset.sha256,
            "native resource SHA-256 mismatch: {}",
            path.display()
        );
    }
    Ok(())
}
