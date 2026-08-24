//! Resource and file-format compatibility for the original Stella client.

pub mod archive;
pub mod crypto;
pub mod ka3d;
pub mod lua;
pub mod native_image;
pub mod pvr;
pub mod surface_format;

use std::path::PathBuf;

use thiserror::Error;

pub use archive::{ArchiveEntry, unpack_7z};
pub use crypto::{
    PERSISTENT_LUA_KEY, RESOURCE_KEY, SEVEN_Z_SIGNATURE, TEXT_FILE_KEY, decrypt_persistent_lua,
    decrypt_resource, decrypt_resource_data, decrypt_text_file, encrypt_persistent_lua,
};

#[derive(Debug, Error)]
pub enum AssetError {
    #[error("encrypted resource length {0} is not a non-zero multiple of 16")]
    InvalidCiphertextLength(usize),
    #[error("AES-CBC padding is invalid")]
    InvalidPadding,
    #[error("decrypted resource is not a 7z archive")]
    NotSevenZip,
    #[error("7z archive error: {0}")]
    Archive(#[from] sevenz_rust2::Error),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("invalid PVR v2 file: {0}")]
    InvalidPvr(&'static str),
    #[error("unsupported PVR v2 pixel format 0x{0:02x}")]
    UnsupportedPvr(u8),
    #[error("invalid PNG image: {0}")]
    InvalidPng(&'static str),
    #[error("unsupported PNG bit depth {0}")]
    UnsupportedPngBitDepth(u8),
    #[error("unsupported PNG color type {0}")]
    UnsupportedPngColorType(u8),
    #[error("invalid WebP image: feature probe failed")]
    InvalidWebp,
    #[error("invalid KA3D resource: {0}")]
    InvalidKa3d(&'static str),
    #[error("invalid Lua chunk: {0}")]
    InvalidLua(&'static str),
    #[error("unsafe archive entry path: {0}")]
    UnsafeArchivePath(String),
}

/// A source file after applying the same resource pipeline as the game.
#[derive(Debug, Clone)]
pub enum DecodedResource {
    /// Plain resources (PVR, WebP, MP3, KA3D metadata, shaders, etc.).
    Plain(Vec<u8>),
    /// AES/7z-wrapped resources. Most archives contain exactly one entry.
    Archive(Vec<ArchiveEntry>),
}

impl DecodedResource {
    pub fn is_archive(&self) -> bool {
        matches!(self, Self::Archive(_))
    }
}

/// Attempt to decode a resource. Files that do not match the encrypted wrapper
/// are returned unchanged.
pub fn decode_resource(input: &[u8]) -> Result<DecodedResource, AssetError> {
    if input.is_empty() || !input.len().is_multiple_of(16) {
        return Ok(DecodedResource::Plain(input.to_vec()));
    }

    let decrypted = match crypto::decrypt_resource(input) {
        Ok(bytes) => bytes,
        Err(AssetError::InvalidPadding | AssetError::NotSevenZip) => {
            return Ok(DecodedResource::Plain(input.to_vec()));
        }
        Err(error) => return Err(error),
    };
    Ok(DecodedResource::Archive(archive::unpack_7z(&decrypted)?))
}

/// Validate an archive entry and resolve it below `base`.
pub fn safe_archive_path(base: &std::path::Path, name: &str) -> Result<PathBuf, AssetError> {
    use std::path::{Component, Path};

    let normalized = name.replace('\\', "/");
    let mut output = base.to_path_buf();
    for component in Path::new(&normalized).components() {
        match component {
            Component::Normal(part) => output.push(part),
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(AssetError::UnsafeArchivePath(name.to_owned()));
            }
        }
    }
    Ok(output)
}
