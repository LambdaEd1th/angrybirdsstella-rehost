//! Process-global LabelPool hash, FIFO lifetime, and deferred texture identity.

use std::{
    collections::{HashMap, VecDeque},
    sync::Arc,
};

use anyhow::{Result, anyhow};
use image::RgbaImage;
use stella_assets::surface_format::SurfaceFormat;

use super::{SystemFontRenderBinding, TextureAsset};

pub(super) const LABEL_POOL_BYTE_LIMIT: u64 = 0x50_0000;

#[derive(Clone)]
pub(crate) struct CachedSystemLabel {
    pub(crate) texture_key: String,
    pub(crate) texture: Arc<TextureAsset>,
}

/// Logical equivalent of Purple's process-global `game::LabelPool`.
///
/// The native pool owns one hash tree, a newest-first insertion vector and a
/// five-mebibyte byte counter. `label_pool_epoch` represents the destructor's
/// last-instance clear; it is not part of Purple's recovered hash.
#[derive(Default)]
pub(crate) struct SystemLabelPool {
    active_epoch: Option<u64>,
    pub(super) bytes: u64,
    pub(super) newest_first: VecDeque<u64>,
    pub(super) labels: HashMap<u64, CachedSystemLabel>,
    next_texture_identity: u64,
}

impl SystemLabelPool {
    pub(crate) fn enter_epoch(&mut self, epoch: u64) -> Vec<String> {
        if self.active_epoch == Some(epoch) {
            return Vec::new();
        }
        let retired = self
            .labels
            .drain()
            .map(|(_, label)| label.texture_key)
            .collect();
        self.bytes = 0;
        self.newest_first.clear();
        self.active_epoch = Some(epoch);
        retired
    }

    pub(crate) fn get(&self, hash: u64) -> Option<CachedSystemLabel> {
        // Purple does not touch the insertion vector on a cache hit. This is
        // FIFO by insertion time, not LRU.
        self.labels.get(&hash).cloned()
    }

    pub(crate) fn insert(
        &mut self,
        epoch: u64,
        hash: u64,
        image: RgbaImage,
    ) -> Result<(CachedSystemLabel, Vec<String>)> {
        debug_assert_eq!(self.active_epoch, Some(epoch));
        if let Some(label) = self.get(hash) {
            return Ok((label, Vec::new()));
        }
        let label_bytes = u64::from(image.width())
            .saturating_mul(u64::from(image.height()))
            .saturating_mul(4);
        if label_bytes > LABEL_POOL_BYTE_LIMIT {
            return Err(anyhow!(
                "system label consumes {label_bytes} bytes, exceeding Purple's 5 MiB LabelPool"
            ));
        }

        let mut retired = Vec::new();
        while self.bytes + label_bytes > LABEL_POOL_BYTE_LIMIT {
            let oldest_hash = self
                .newest_first
                .pop_back()
                .ok_or_else(|| anyhow!("native LabelPool FIFO lost its oldest entry"))?;
            let oldest = self
                .labels
                .remove(&oldest_hash)
                .ok_or_else(|| anyhow!("native LabelPool hash tree lost FIFO entry"))?;
            self.bytes -=
                u64::from(oldest.texture.width()) * u64::from(oldest.texture.height()) * 4;
            retired.push(oldest.texture_key);
        }

        let identity = self.next_texture_identity;
        self.next_texture_identity = self.next_texture_identity.wrapping_add(1);
        let texture_key = format!("<system-font:{hash:016x}@{epoch:016x}#{identity:016x}>");
        let label = CachedSystemLabel {
            texture_key,
            texture: Arc::new(TextureAsset::new(image, SurfaceFormat::A8B8G8R8)),
        };
        self.bytes += label_bytes;
        self.labels.insert(hash, label.clone());
        // The recovered vector inserts at begin(); eviction removes end()-1.
        self.newest_first.push_front(hash);
        Ok((label, retired))
    }
}

#[cfg(test)]
pub(super) fn system_label_lifetime_key(binding: &SystemFontRenderBinding, text: &str) -> String {
    let hash = native_system_label_hash(binding, text);
    // The suffix is a deferred-host identity only. Purple can reuse the same
    // numeric hash after clearing its global pool because prior GL submissions
    // have already consumed the old Label pointer; wgpu prepares all commands
    // later and therefore has to keep both lifetimes addressable at once.
    format!(
        "<system-font:{hash:016x}@{:016x}>",
        binding.label_pool_epoch
    )
}

pub(crate) fn native_system_label_hash(binding: &SystemFontRenderBinding, text: &str) -> u64 {
    // Exact DJB2-style field order recovered at 0x100475D18..0x100475D8C.
    let mut hash = 5381_u64;
    for byte in binding.family.bytes() {
        hash = hash.wrapping_mul(33).wrapping_add(u64::from(byte));
    }
    hash = hash.wrapping_mul(33);
    for byte in text.bytes() {
        hash = hash.wrapping_add(u64::from(byte)).wrapping_mul(33);
    }
    for value in [
        binding.size,
        i32::from_be_bytes([
            binding.fill_rgba[3],
            binding.fill_rgba[0],
            binding.fill_rgba[1],
            binding.fill_rgba[2],
        ]),
        binding.stroke_width,
        i32::from_be_bytes([
            binding.stroke_rgba[3],
            binding.stroke_rgba[0],
            binding.stroke_rgba[1],
            binding.stroke_rgba[2],
        ]),
    ] {
        // The ARM64 callers use SXTW for every signed int field before the
        // 64-bit wrapping additions. Opaque AARRGGBB colors are therefore
        // negative operands rather than zero-extended u32 values.
        hash = hash.wrapping_add(i64::from(value) as u64).wrapping_mul(33);
    }
    hash = hash.wrapping_add(i64::from(binding.style) as u64);
    hash
}
