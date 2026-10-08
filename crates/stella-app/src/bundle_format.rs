//! Deterministic desktop resource manifest, shared by the builder and launcher.

use std::{io, io::Read, io::Write};

use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub(crate) const FORMAT_VERSION: u32 = 1;
pub(crate) const BUNDLE_MAGIC: &[u8] = b"STELLA-DESKTOP-RESOURCE-BUNDLE-V1\0";
pub(crate) const INSTALLED_MANIFEST: &str = ".stella-resource-manifest.json";

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Manifest {
    pub(crate) version: u32,
    pub(crate) files: Vec<ResourceFile>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ResourceFile {
    pub(crate) path: String,
    pub(crate) size: u64,
    pub(crate) sha256: [u8; 32],
}

impl Manifest {
    pub(crate) fn validate(&self) -> Result<()> {
        ensure!(
            self.version == FORMAT_VERSION,
            "unsupported resource bundle version"
        );
        ensure!(!self.files.is_empty(), "resource bundle is empty");
        let mut previous = None;
        for file in &self.files {
            validate_relative_path(&file.path)?;
            ensure!(file.path != INSTALLED_MANIFEST, "reserved resource path");
            if let Some(previous) = previous {
                ensure!(
                    previous < file.path.as_str(),
                    "unsorted or duplicate resource path"
                );
            }
            previous = Some(file.path.as_str());
        }
        Ok(())
    }
}

pub(crate) fn validate_relative_path(path: &str) -> Result<()> {
    ensure!(
        !path.is_empty() && path.len() <= 4096,
        "invalid resource path length"
    );
    for component in path.split('/') {
        ensure!(
            !component.is_empty()
                && component != "."
                && component != ".."
                && !component.ends_with(['.', ' '])
                && !component
                    .chars()
                    .any(|c| c.is_control() || "\\:*?\"<>|".contains(c)),
            "unsafe resource path: {path}"
        );
        let stem = component
            .split('.')
            .next()
            .unwrap_or_default()
            .to_ascii_uppercase();
        ensure!(
            !matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
                && !(stem.len() == 4
                    && (stem.starts_with("COM") || stem.starts_with("LPT"))
                    && matches!(stem.as_bytes()[3], b'1'..=b'9')),
            "Windows-reserved resource path: {path}"
        );
    }
    Ok(())
}

pub(crate) fn copy_hashed(
    reader: &mut impl Read,
    writer: &mut impl Write,
) -> io::Result<(u64, [u8; 32])> {
    let mut hash = Sha256::new();
    let mut size = 0;
    let mut buffer = [0; 64 * 1024];
    loop {
        let count = match reader.read(&mut buffer) {
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            result => result?,
        };
        if count == 0 {
            break;
        }
        writer.write_all(&buffer[..count])?;
        hash.update(&buffer[..count]);
        size += count as u64;
    }
    Ok((size, hash.finalize().into()))
}

pub(crate) fn hex_digest(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    bytes
        .iter()
        .flat_map(|byte| {
            [
                char::from(HEX[(byte >> 4) as usize]),
                char::from(HEX[(byte & 15) as usize]),
            ]
        })
        .collect()
}
