//! SpriteSheet texture payload accounting owned by the native sheet loader.

use std::{fs, path::Path};

use stella_assets::{ka3d::SpriteSheet, pvr::parse_header};

use crate::resolve_data_file;

#[derive(Debug)]
pub(in crate::resource_manager) struct SheetTexture {
    pub(in crate::resource_manager) cache_key: String,
    pub(in crate::resource_manager) uploaded_bytes: u32,
}

pub(in crate::resource_manager) fn sprite_sheet_textures(
    data_root: &Path,
    requested: &str,
) -> Vec<SheetTexture> {
    let Ok(sheet_path) = resolve_data_file(data_root, requested) else {
        return Vec::new();
    };
    let Ok(bytes) = fs::read(&sheet_path) else {
        return Vec::new();
    };
    let Ok(sheet) = SpriteSheet::parse(&bytes) else {
        return Vec::new();
    };
    let Some(parent) = sheet_path.parent() else {
        return Vec::new();
    };

    sheet
        .textures
        .into_iter()
        .filter_map(|texture| {
            let texture_path = parent.join(texture.trim_start_matches('/'));
            let bytes = fs::read(&texture_path).ok()?;
            let header = parse_header(&bytes).ok()?;
            Some(SheetTexture {
                cache_key: texture_path.to_string_lossy().into_owned(),
                uploaded_bytes: header.data_length,
            })
        })
        .collect()
}
