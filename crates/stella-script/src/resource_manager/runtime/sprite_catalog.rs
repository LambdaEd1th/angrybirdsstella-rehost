//! Active sprite lookup, retained composite pointers and deferred wgpu catalog export.

use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
};

use stella_assets::ka3d::CompositePart;

use super::{ResourceRuntime, SpriteResourceEntry, SpriteResourceKind};
use crate::{
    BoundCompositePart, SpriteCatalogRegion, SpriteCatalogSnapshot,
    resource_manager::{NativeSpriteMetrics, SpriteGeometry, native_composite_metrics},
};

impl ResourceRuntime {
    /// Resolve the host path and atlas-region bindings once, at the same
    /// lifetime boundary where Purple constructs its SpriteSheet resources.
    /// Native draw calls retain pointers to those resources; they do not
    /// canonicalize the texture filename again for every submitted sprite.
    pub(crate) fn cache_sprite_sheet_host_bindings(&mut self, owner: &str, data_root: &Path) {
        let Some(sheet) = self.sprite_sheet_values.get(owner) else {
            return;
        };
        let descriptor = self.sprite_sheet_descriptor_paths.get(owner);
        let texture_sources = sheet
            .textures
            .iter()
            .map(|texture| {
                (
                    texture.clone(),
                    resolve_texture_source(data_root, descriptor, texture),
                )
            })
            .collect::<BTreeMap<_, _>>();
        let native_sheet_id = self
            .sprite_sheet_identities
            .get(owner)
            .copied()
            .unwrap_or(0);
        let regions = sheet
            .sprites
            .iter()
            .filter_map(|sprite| {
                let texture = sheet.texture_for(sprite)?;
                Some((
                    sprite.name.clone(),
                    SpriteCatalogRegion {
                        native_sheet_id,
                        texture_source: texture_sources.get(texture)?.clone(),
                        sprite: sprite.clone(),
                    },
                ))
            })
            .collect::<BTreeMap<_, _>>();
        self.sprite_sheet_texture_sources
            .insert(owner.to_owned(), texture_sources);
        self.sprite_sheet_catalog_regions
            .insert(owner.to_owned(), regions);
    }

    pub(super) fn sprite_sheet_catalog_region(
        &self,
        owner: &str,
        name: &str,
        data_root: &Path,
    ) -> Option<SpriteCatalogRegion> {
        if let Some(region) = self
            .sprite_sheet_catalog_regions
            .get(owner)
            .and_then(|regions| regions.get(name))
        {
            return Some(region.clone());
        }
        // Direct ResourceRuntime fixtures can install parsed values without
        // going through createSpriteSheet. Keep that diagnostic path working;
        // production loads always populate the cache above.
        let sheet = self.sprite_sheet_values.get(owner)?;
        let sprite = sheet
            .sprites
            .iter()
            .find(|sprite| sprite.name == name)?
            .clone();
        let texture = sheet.texture_for(&sprite)?;
        let descriptor = self.sprite_sheet_descriptor_paths.get(owner);
        Some(SpriteCatalogRegion {
            native_sheet_id: self
                .sprite_sheet_identities
                .get(owner)
                .copied()
                .unwrap_or(0),
            texture_source: resolve_texture_source(data_root, descriptor, texture),
            sprite,
        })
    }

    pub(crate) fn sprite_catalog_snapshot(&self, data_root: &Path) -> SpriteCatalogSnapshot {
        let mut regions = BTreeMap::new();
        let mut composites = BTreeMap::new();
        let mut masked_textures = BTreeMap::new();
        for (name, entries) in &self.sprite_entries {
            let Some(entry) = entries.last() else {
                continue;
            };
            match entry.kind {
                SpriteResourceKind::Atlas => {
                    let Some(sheet) = self.sprite_sheet_values.get(&entry.owner) else {
                        continue;
                    };
                    let Some(region) =
                        self.sprite_sheet_catalog_region(&entry.owner, name, data_root)
                    else {
                        continue;
                    };
                    let texture_source = region.texture_source.clone();
                    regions.insert(name.clone(), region);
                    if sheet.sprites.len() == 1 {
                        masked_textures.insert(entry.owner.clone(), texture_source);
                    }
                }
                SpriteResourceKind::Composite => {
                    let Some((parts, part_regions)) = self.active_composite_bound_parts(name)
                    else {
                        continue;
                    };
                    let bound_parts = parts
                        .iter()
                        .zip(part_regions)
                        .enumerate()
                        .map(|(index, (part, region))| {
                            let alias = format!("<composite:{}/{}/{}>", entry.owner, name, index);
                            regions.insert(alias.clone(), region.clone());
                            let mut part = part.clone();
                            part.sprite = alias;
                            part
                        })
                        .collect();
                    composites.insert(name.clone(), bound_parts);
                }
            }
        }
        SpriteCatalogSnapshot {
            revision: self.sprite_catalog_revision,
            regions,
            composites,
            masked_textures,
        }
    }

    pub(crate) fn active_sprite_entry(
        &self,
        name: &str,
        required_kind: Option<SpriteResourceKind>,
    ) -> Option<&SpriteResourceEntry> {
        let name = name.split_once('#').map_or(name, |(base, _)| base);
        let entry = self.sprite_entries.get(name)?.last()?;
        required_kind
            .is_none_or(|required| required == entry.kind)
            .then_some(entry)
    }

    pub(crate) fn active_atlas_geometry(&self, name: &str) -> Option<SpriteGeometry> {
        let asset_name = name.split_once('#').map_or(name, |(base, _)| base);
        let entry = self.active_sprite_entry(asset_name, Some(SpriteResourceKind::Atlas))?;
        let sprite = self
            .sprite_sheet_values
            .get(&entry.owner)?
            .sprites
            .iter()
            .find(|sprite| sprite.name == asset_name)?;
        Some(SpriteGeometry {
            min_x: -f64::from(sprite.pivot_x),
            min_y: -f64::from(sprite.pivot_y),
            max_x: f64::from(sprite.width) - f64::from(sprite.pivot_x),
            max_y: f64::from(sprite.height) - f64::from(sprite.pivot_y),
        })
    }

    pub(crate) fn active_atlas_catalog_region(
        &self,
        name: &str,
        data_root: &Path,
    ) -> Option<SpriteCatalogRegion> {
        let asset_name = name.split_once('#').map_or(name, |(base, _)| base);
        let entry = self.active_sprite_entry(asset_name, Some(SpriteResourceKind::Atlas))?;
        self.sprite_sheet_catalog_region(&entry.owner, asset_name, data_root)
    }

    pub(crate) fn active_masked_texture_source(
        &self,
        name: &str,
        data_root: &Path,
    ) -> Option<String> {
        let direct = self.sprite_sheet_values.contains_key(name).then_some(name);
        let normalized;
        let owner = if let Some(owner) = direct {
            owner
        } else {
            normalized = crate::resource_manager::resource_double_file_stem(name);
            normalized.as_str()
        };
        if self.released_sprite_sheet_resources.contains(owner) {
            return None;
        }
        let sheet = self.sprite_sheet_values.get(owner)?;
        let texture = sheet.current_texture()?;
        self.sprite_sheet_texture_sources
            .get(owner)
            .and_then(|textures| textures.get(texture))
            .cloned()
            .or_else(|| {
                let descriptor = self.sprite_sheet_descriptor_paths.get(owner);
                Some(resolve_texture_source(data_root, descriptor, texture))
            })
    }

    pub(crate) fn active_composite_parts(&self, name: &str) -> Option<&[CompositePart]> {
        let asset_name = name.split_once('#').map_or(name, |(base, _)| base);
        let entry = self.active_sprite_entry(asset_name, Some(SpriteResourceKind::Composite))?;
        self.composite_set_values
            .get(&entry.owner)?
            .sprites
            .iter()
            .find(|sprite| sprite.name == asset_name)
            .map(|sprite| sprite.parts.as_slice())
    }

    pub(crate) fn active_composite_bound_parts(
        &self,
        name: &str,
    ) -> Option<(&[CompositePart], &[SpriteCatalogRegion])> {
        let asset_name = name.split_once('#').map_or(name, |(base, _)| base);
        let entry = self.active_sprite_entry(asset_name, Some(SpriteResourceKind::Composite))?;
        let parts = self
            .composite_set_values
            .get(&entry.owner)?
            .sprites
            .iter()
            .find(|sprite| sprite.name == asset_name)?
            .parts
            .as_slice();
        let regions = self
            .composite_set_regions
            .get(&entry.owner)?
            .get(asset_name)?
            .as_slice();
        (parts.len() == regions.len()).then_some((parts, regions))
    }

    pub(crate) fn active_bound_composite(&self, name: &str) -> Option<Vec<BoundCompositePart>> {
        let (parts, regions) = self.active_composite_bound_parts(name)?;
        Some(
            parts
                .iter()
                .cloned()
                .zip(regions.iter().cloned())
                .map(|(part, region)| BoundCompositePart { part, region })
                .collect(),
        )
    }

    pub(crate) fn active_composite_parts_mut(
        &mut self,
        name: &str,
    ) -> Option<&mut Vec<CompositePart>> {
        let asset_name = name.split_once('#').map_or(name, |(base, _)| base);
        let owner = self
            .active_sprite_entry(asset_name, Some(SpriteResourceKind::Composite))?
            .owner
            .clone();
        self.composite_set_values
            .get_mut(&owner)?
            .sprites
            .iter_mut()
            .find(|sprite| sprite.name == asset_name)
            .map(|sprite| &mut sprite.parts)
    }

    pub(crate) fn active_geometry(&self, name: &str) -> Option<SpriteGeometry> {
        self.resolve_active_geometry(name, &mut BTreeSet::new())
    }

    /// Return the concrete integer fields used by Purple's three native
    /// Sprite queries (`getBoundsX/Y` and `getPivotX/Y`). AtlasSprite stores
    /// these values directly, while CompoSprite rebuilds them with the
    /// transformed-FCVTZS pass in `sub_100436D40`.
    pub(crate) fn active_native_sprite_metrics(&self, name: &str) -> Option<NativeSpriteMetrics> {
        let asset_name = name.split_once('#').map_or(name, |(base, _)| base);
        let entry = self.active_sprite_entry(asset_name, None)?;
        match entry.kind {
            SpriteResourceKind::Atlas => {
                let sprite = self
                    .sprite_sheet_values
                    .get(&entry.owner)?
                    .sprites
                    .iter()
                    .find(|sprite| sprite.name == asset_name)?;
                Some(NativeSpriteMetrics {
                    width: i32::from(sprite.width),
                    height: i32::from(sprite.height),
                    pivot_x: i32::from(sprite.pivot_x),
                    pivot_y: i32::from(sprite.pivot_y),
                })
            }
            SpriteResourceKind::Composite => {
                let parts = self.active_bound_composite(asset_name)?;
                Some(
                    native_composite_metrics(&parts).unwrap_or(NativeSpriteMetrics {
                        width: 0,
                        height: 0,
                        pivot_x: 0,
                        pivot_y: 0,
                    }),
                )
            }
        }
    }

    fn resolve_active_geometry(
        &self,
        name: &str,
        visiting: &mut BTreeSet<String>,
    ) -> Option<SpriteGeometry> {
        let asset_name = name.split_once('#').map_or(name, |(base, _)| base);
        let entry = self.active_sprite_entry(asset_name, None)?;
        match entry.kind {
            SpriteResourceKind::Atlas => self.active_atlas_geometry(asset_name),
            SpriteResourceKind::Composite => {
                if !visiting.insert(asset_name.to_owned()) {
                    return None;
                }
                let parts = self.active_composite_parts(asset_name)?;
                let mut result: Option<SpriteGeometry> = None;
                for part in parts.iter().filter(|part| part.visible) {
                    let child = self
                        .resolve_active_geometry(&part.sprite, visiting)?
                        .transformed(part);
                    match &mut result {
                        Some(current) => current.include(child),
                        None => result = Some(child),
                    }
                }
                visiting.remove(asset_name);
                result
            }
        }
    }
}

pub(super) fn resolve_texture_source(
    data_root: &Path,
    descriptor: Option<&PathBuf>,
    texture: &str,
) -> String {
    let requested = Path::new(texture);
    let mut candidates = Vec::new();
    if requested.is_absolute() {
        candidates.push(requested.to_path_buf());
    } else {
        if let Some(parent) = descriptor.and_then(|path| path.parent()) {
            candidates.push(parent.join(requested));
        }
        candidates.push(crate::app_data_root(data_root).join(requested));
        candidates.push(data_root.join(requested));
        candidates.push(data_root.join("images/1024x768").join(requested));
        candidates.push(data_root.join("fonts/1024x768").join(requested));
    }
    let selected = candidates
        .iter()
        .find(|path| path.is_file())
        .cloned()
        .or_else(|| candidates.into_iter().next())
        .unwrap_or_else(|| requested.to_path_buf());
    selected
        .canonicalize()
        .unwrap_or(selected)
        .to_string_lossy()
        .into_owned()
}
