use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicUsize, Ordering},
};

use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand};
use rayon::prelude::*;
use stella_assets::{
    DecodedResource, decode_resource,
    ka3d::{BitmapFont, CompositeSpriteSet, Ka3dEnvelope, LocalizationTable, SpriteSheet},
    lua::LuaChunkHeader,
};
use walkdir::WalkDir;

#[derive(Debug, Parser)]
#[command(
    name = "stella-tool",
    about = "Inspect and extract Purple 1.1.6 resources"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Inspect one source or extracted resource.
    Inspect { path: PathBuf },
    /// Decode one encrypted file into a directory.
    Decode {
        path: PathBuf,
        #[arg(short, long)]
        output: PathBuf,
    },
    /// Decode encrypted files and copy plain files from a source tree.
    Extract {
        #[arg(short, long)]
        source: PathBuf,
        #[arg(short, long)]
        output: PathBuf,
    },
    /// Verify every file in a source tree without writing output.
    Verify { source: PathBuf },
    /// Convert one PVR v2 RGBA4444/RGBA8888 texture to PNG.
    PvrToPng { input: PathBuf, output: PathBuf },
    /// Rewrite Purple's Lua 5.1 bytecode representation for the current host.
    TranscodeLua { input: PathBuf, output: PathBuf },
    /// Wrap host Lua 5.1 bytecode in loadable Lua text without changing its chunk.
    WrapLuaText {
        input: PathBuf,
        output: PathBuf,
        #[arg(long)]
        chunk_name: Option<String>,
    },
    /// Recover host Lua 5.1 bytecode from a lossless text wrapper.
    UnwrapLuaText { input: PathBuf, output: PathBuf },
}

fn main() -> Result<()> {
    match Cli::parse().command {
        Command::Inspect { path } => inspect(&path),
        Command::Decode { path, output } => decode_one(&path, &output),
        Command::Extract { source, output } => extract_tree(&source, &output),
        Command::Verify { source } => verify_tree(&source),
        Command::PvrToPng { input, output } => {
            let bytes = fs::read(&input).with_context(|| format!("reading {}", input.display()))?;
            stella_assets::pvr::save_png(&bytes, &output)
                .with_context(|| format!("converting {}", input.display()))
        }
        Command::TranscodeLua { input, output } => {
            let bytes = fs::read(&input).with_context(|| format!("reading {}", input.display()))?;
            let transcoded = stella_assets::lua::transcode_for_host(&bytes)?;
            write_file(&output, &transcoded)
        }
        Command::WrapLuaText {
            input,
            output,
            chunk_name,
        } => wrap_lua_text(&input, &output, chunk_name.as_deref()),
        Command::UnwrapLuaText { input, output } => {
            let bytes = fs::read(&input).with_context(|| format!("reading {}", input.display()))?;
            let unwrapped =
                stella_assets::lua::unwrap_host_chunk_text(&bytes)?.with_context(|| {
                    format!("{} is not a lossless Lua text wrapper", input.display())
                })?;
            write_file(&output, &unwrapped)
        }
    }
}

fn wrap_lua_text(input: &Path, output: &Path, chunk_name: Option<&str>) -> Result<()> {
    let bytes = fs::read(input).with_context(|| format!("reading {}", input.display()))?;
    let chunk_name = chunk_name
        .map(str::to_owned)
        .unwrap_or_else(|| format!("@{}", input.display()));
    let source = stella_assets::lua::wrap_host_chunk_as_text(&bytes, &chunk_name)?;
    write_file(output, source.as_bytes())
}

fn inspect(path: &Path) -> Result<()> {
    let bytes = fs::read(path).with_context(|| format!("reading {}", path.display()))?;
    match decode_resource(&bytes)? {
        DecodedResource::Archive(entries) => {
            println!("encrypted archive: {} entries", entries.len());
            for entry in entries {
                println!(
                    "  {} ({} bytes): {}",
                    entry.name,
                    entry.bytes.len(),
                    classify(&entry.bytes)
                );
            }
        }
        DecodedResource::Plain(bytes) => {
            println!(
                "plain resource ({} bytes): {}",
                bytes.len(),
                classify(&bytes)
            );
            inspect_ka3d(&bytes)?;
        }
    }
    Ok(())
}

fn inspect_ka3d(bytes: &[u8]) -> Result<()> {
    if Ka3dEnvelope::find(bytes, b"SPRT").is_ok() {
        let sheet = SpriteSheet::parse(bytes)?;
        println!("SPRT textures: {}", sheet.textures.join(", "));
        for sprite in sheet.sprites {
            println!(
                "  {}: atlas=({}, {}) size={}x{} pivot=({}, {})",
                sprite.name,
                sprite.x,
                sprite.y,
                sprite.width,
                sprite.height,
                sprite.pivot_x,
                sprite.pivot_y
            );
        }
    } else if Ka3dEnvelope::find(bytes, b"COMP").is_ok() {
        let set = CompositeSpriteSet::parse(bytes)?;
        for sprite in set.sprites {
            println!("COMP {}: {} parts", sprite.name, sprite.parts.len());
            for part in sprite.parts {
                println!(
                    "  {}: position=({}, {}) scale=({}, {}) flip=({}, {}) angle={}rad",
                    part.sprite,
                    part.x,
                    part.y,
                    part.scale_x,
                    part.scale_y,
                    part.flip_x,
                    part.flip_y,
                    part.angle
                );
            }
        }
    } else if Ka3dEnvelope::find(bytes, b"FONT").is_ok() {
        let font = BitmapFont::parse(bytes)?;
        println!(
            "FONT texture: {}, glyphs: {}, leading: {}, tracking: {}",
            font.texture,
            font.glyphs.len(),
            font.leading,
            font.tracking
        );
    } else if Ka3dEnvelope::find(bytes, b"TEXT").is_ok() {
        let table = LocalizationTable::parse(bytes)?;
        println!(
            "TEXT locales: {}, ids: {}, groups: {}",
            table.locales.len(),
            table.ids.len(),
            table.translations.len()
        );
    }
    Ok(())
}

fn decode_one(path: &Path, output: &Path) -> Result<()> {
    let bytes = fs::read(path).with_context(|| format!("reading {}", path.display()))?;
    let DecodedResource::Archive(entries) = decode_resource(&bytes)? else {
        bail!("{} is not an encrypted Stella resource", path.display());
    };
    for entry in entries {
        let destination = stella_assets::safe_archive_path(output, &entry.name)?;
        write_file(&destination, &entry.bytes)?;
        println!("{}", destination.display());
    }
    Ok(())
}

fn source_files(source: &Path) -> Result<Vec<PathBuf>> {
    if !source.is_dir() {
        bail!("source is not a directory: {}", source.display());
    }
    Ok(WalkDir::new(source)
        .follow_links(false)
        .into_iter()
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.file_type().is_file())
        .map(|entry| entry.into_path())
        .collect())
}

fn extract_tree(source: &Path, output: &Path) -> Result<()> {
    let files = source_files(source)?;
    let encrypted = AtomicUsize::new(0);
    let plain = AtomicUsize::new(0);
    let written = AtomicUsize::new(0);

    files.par_iter().try_for_each(|path| -> Result<()> {
        let relative = path.strip_prefix(source).unwrap();
        let bytes = fs::read(path).with_context(|| format!("reading {}", path.display()))?;
        match decode_resource(&bytes)? {
            DecodedResource::Plain(bytes) => {
                write_file(&output.join(relative), &bytes)?;
                plain.fetch_add(1, Ordering::Relaxed);
                written.fetch_add(1, Ordering::Relaxed);
            }
            DecodedResource::Archive(entries) => {
                let parent = relative.parent().unwrap_or(Path::new(""));
                for entry in entries {
                    let destination =
                        stella_assets::safe_archive_path(&output.join(parent), &entry.name)?;
                    write_file(&destination, &entry.bytes)?;
                    written.fetch_add(1, Ordering::Relaxed);
                }
                encrypted.fetch_add(1, Ordering::Relaxed);
            }
        }
        Ok(())
    })?;

    println!(
        "extracted {} source files: {} encrypted, {} plain, {} outputs",
        files.len(),
        encrypted.load(Ordering::Relaxed),
        plain.load(Ordering::Relaxed),
        written.load(Ordering::Relaxed)
    );
    Ok(())
}

fn verify_tree(source: &Path) -> Result<()> {
    let files = source_files(source)?;
    let encrypted = AtomicUsize::new(0);
    let lua = AtomicUsize::new(0);
    let json = AtomicUsize::new(0);
    let ka3d = AtomicUsize::new(0);
    let pvr = AtomicUsize::new(0);

    files.par_iter().try_for_each(|path| -> Result<()> {
        let bytes = fs::read(path)?;
        match decode_resource(&bytes)? {
            DecodedResource::Archive(entries) => {
                encrypted.fetch_add(1, Ordering::Relaxed);
                for entry in entries {
                    count_kind(&entry.bytes, &lua, &json, &ka3d, &pvr);
                }
            }
            DecodedResource::Plain(bytes) => count_kind(&bytes, &lua, &json, &ka3d, &pvr),
        }
        Ok(())
    })?;

    println!("files: {}", files.len());
    println!("encrypted archives: {}", encrypted.load(Ordering::Relaxed));
    println!("Lua 5.1 chunks: {}", lua.load(Ordering::Relaxed));
    println!("JSON documents: {}", json.load(Ordering::Relaxed));
    println!("KA3D envelopes: {}", ka3d.load(Ordering::Relaxed));
    println!("PVR v2 textures: {}", pvr.load(Ordering::Relaxed));
    Ok(())
}

fn count_kind(
    bytes: &[u8],
    lua: &AtomicUsize,
    json: &AtomicUsize,
    ka3d: &AtomicUsize,
    pvr: &AtomicUsize,
) {
    let kind = classify(bytes);
    match kind {
        "Lua 5.1 bytecode" => lua.fetch_add(1, Ordering::Relaxed),
        "JSON" => json.fetch_add(1, Ordering::Relaxed),
        "KA3D" => ka3d.fetch_add(1, Ordering::Relaxed),
        "PVR v2" => pvr.fetch_add(1, Ordering::Relaxed),
        _ => 0,
    };
}

fn classify(bytes: &[u8]) -> &'static str {
    if LuaChunkHeader::parse(bytes).is_ok() {
        "Lua 5.1 bytecode"
    } else if serde_json::from_slice::<serde_json::Value>(bytes).is_ok() {
        "JSON"
    } else if Ka3dEnvelope::parse(bytes).is_ok() {
        "KA3D"
    } else if stella_assets::pvr::parse_header(bytes).is_ok() {
        "PVR v2"
    } else if bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WEBP") {
        "WebP"
    } else if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        "PNG"
    } else if bytes.starts_with(b"ID3") || bytes.starts_with(&[0xff, 0xfb]) {
        "MP3"
    } else if bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WAVE") {
        "WAV"
    } else if std::str::from_utf8(bytes).is_ok() {
        "text"
    } else {
        "binary"
    }
}

fn write_file(path: &Path, bytes: &[u8]) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).with_context(|| format!("creating {}", parent.display()))?;
    }
    fs::write(path, bytes).with_context(|| format!("writing {}", path.display()))
}
