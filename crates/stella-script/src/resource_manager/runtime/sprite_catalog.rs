//! Active sprite lookup, retained composite pointers and deferred wgpu catalog export.

use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
    sync::Arc,
};

use stella_assets::image_source::sheet_image_source;
use stella_assets::ka3d::CompositePart;

use super::file_images::resolve_file_image_path;
use super::{PreparedSheetImages, ResourceRuntime, SpriteResourceEntry, SpriteResourceKind};
use crate::{
    BoundCompositePart, CompositeSpriteOwner, MaskedTextureBinding, SharedSpriteName,
    SpriteCatalogRegion, SpriteCatalogSnapshot,
    resource_manager::{NativeSpriteMetrics, SpriteGeometry, native_composite_metrics_from_parts},
};

impl ResourceRuntime {
    /// Publish the BitmapFont atlas after successful construction. Purple
    /// creates and retains the texture and
    /// AtlasSprite owner in `sub_10042A780`; draw never reopens this path.
    pub(crate) fn publish_bitmap_font_host_binding(
        &mut self,
        owner: &str,
        image: Option<crate::SheetImageSnapshot>,
    ) {
        self.bitmap_font_image_owners.remove(owner);
        self.bitmap_font_decoded_images.remove(owner);
        let texture_source = if let Some(image) = image {
            self.bitmap_font_image_owners
                .insert(owner.to_owned(), image.owner);
            self.bitmap_font_decoded_images.insert(
                owner.to_owned(),
                image.image.expect("constructed file Image"),
            );
            image.source
        } else {
            String::new()
        };
        self.bitmap_font_texture_sources
            .insert(owner.to_owned(), texture_source);
        self.register_bitmap_font_value(owner);
    }

    /// Resolve the host path and atlas-region bindings once, at the same
    /// lifetime boundary where Purple constructs its SpriteSheet resources.
    /// Native draw calls retain pointers to those resources; they do not
    /// canonicalize the texture filename again for every submitted sprite.
    /// Returns the native Texture allocation delta before earlier SPRT Images
    /// are released. Immutable host pixel sharing does not change this counter.
    #[cfg(test)]
    pub(crate) fn cache_sprite_sheet_host_bindings(
        &mut self,
        owner: &str,
        data_root: &Path,
    ) -> crate::LuaResult<u32> {
        let Some(sheet) = self.sprite_sheet_values.get(owner) else {
            return Ok(0);
        };
        let textures = sheet.textures.clone();
        let descriptor = self.sprite_sheet_descriptor_paths.get(owner).cloned();
        let mut prepared = PreparedSheetImages::default();
        for (index, texture) in textures.iter().enumerate() {
            let image =
                self.load_sheet_file_image(data_root, descriptor.as_ref(), texture, index)?;
            prepared.push(image);
        }
        Ok(self.publish_sprite_sheet_host_bindings(owner, prepared))
    }

    /// Publish a complete candidate without file I/O or a fallible decoder.
    pub(crate) fn publish_sprite_sheet_host_bindings(
        &mut self,
        owner: &str,
        prepared: PreparedSheetImages,
    ) -> u32 {
        let Some(current_texture_index) = prepared.texture_sources.len().checked_sub(1) else {
            return 0;
        };
        let texture_sources = prepared.texture_sources;
        let current_image = prepared.current_image.expect("prepared sheet Image");
        let image_owner = current_image.owner.clone();
        let mut decoded_images = vec![None; texture_sources.len()];
        decoded_images[current_texture_index] = current_image.image.clone();
        let image_cell = Arc::clone(
            self.sprite_sheet_image_cells
                .entry(owner.to_owned())
                .or_default(),
        );
        let regions_by_index = {
            let sheet = &self.sprite_sheet_values[owner];
            let native_sheet_id = self
                .sprite_sheet_identities
                .get(owner)
                .copied()
                .unwrap_or(0);
            sheet
                .sprites
                .iter()
                .enumerate()
                .map(|(index, sprite)| {
                    let texture_index = *sheet.sprite_texture_indices.get(index)?;
                    let uv_image_dimensions = prepared
                        .constructor_dimensions
                        .get(texture_index)
                        .copied()
                        .flatten()
                        .or_else(|| {
                            self.sprite_sheet_catalog_regions
                                .get(owner)
                                .and_then(|regions| regions.get(&sprite.name))
                                .and_then(|region| region.uv_image_dimensions)
                        });
                    Some(Arc::new(SpriteCatalogRegion {
                        sheet_image: Some(image_cell.bind()),
                        uv_image_dimensions,
                        image_owner: None,
                        decoded_image: None,
                        native_sheet_id,
                        texture_source: texture_sources.get(current_texture_index)?.clone(),
                        sprite: sprite.clone(),
                    }))
                })
                .collect::<Vec<_>>()
        };

        // Image dimensions belong to the successful sheet construction, not
        // to the active global sprite (which another sheet can shadow).
        if let Some(dimensions) = current_image.dimensions {
            self.sprite_sheet_image_dimensions
                .insert(owner.to_owned(), dimensions);
        }
        image_cell.replace(Some(current_image));
        // Earlier Image allocations have no native owner after replacement.
        // Only their constructor UV extent survives on the AtlasSprite.
        decoded_images[..current_texture_index].fill(None);
        self.sprite_sheet_decoded_images
            .insert(owner.to_owned(), decoded_images);
        self.sprite_sheet_image_owners
            .insert(owner.to_owned(), image_owner);

        // Resources+0x588 stores the concrete Sprite pointer in every stack
        // entry. Bind the same immutable owner once at sheet construction so
        // the hot active lookup needs only the name-tree search and last item.
        for (index, region) in regions_by_index.iter().enumerate() {
            let Some(region) = region else {
                continue;
            };
            let Some(entries) = self.sprite_entries.get_mut(&region.sprite.name) else {
                continue;
            };
            if let Some(entry) = entries.iter_mut().rev().find(|entry| {
                entry.kind == SpriteResourceKind::Atlas
                    && entry.owner == owner
                    && entry.index == index
            }) {
                entry.atlas_region = Some(Arc::clone(region));
            }
        }
        let regions = regions_by_index
            .into_iter()
            .flatten()
            .map(|region| (region.sprite.name.clone(), region))
            .collect::<BTreeMap<_, _>>();
        self.sprite_sheet_texture_sources
            .insert(owner.to_owned(), texture_sources);
        self.sprite_sheet_catalog_regions
            .insert(owner.to_owned(), regions);
        prepared.allocation_bytes
    }

    pub(super) fn sprite_sheet_catalog_region(
        &self,
        owner: &str,
        name: &str,
        data_root: &Path,
    ) -> Option<Arc<SpriteCatalogRegion>> {
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
        let (index, sprite) = sheet
            .sprites
            .iter()
            .enumerate()
            .find(|(_, sprite)| sprite.name == name)?;
        let texture_index = sheet.textures.len().checked_sub(1)?;
        Some(Arc::new(SpriteCatalogRegion {
            sheet_image: None,
            uv_image_dimensions: self
                .sprite_sheet_decoded_images
                .get(owner)
                .and_then(|images| images.get(*sheet.sprite_texture_indices.get(index)?))
                .and_then(Option::as_ref)
                .map(|image| [image.width, image.height]),
            image_owner: self.sprite_sheet_image_owners.get(owner).cloned(),
            decoded_image: self
                .sprite_sheet_decoded_images
                .get(owner)
                .and_then(|images| images.get(texture_index))
                .cloned()
                .flatten(),
            native_sheet_id: self
                .sprite_sheet_identities
                .get(owner)
                .copied()
                .unwrap_or(0),
            texture_source: self.sprite_sheet_image_source(owner, texture_index, data_root)?,
            sprite: sprite.clone(),
        }))
    }

    pub(crate) fn sprite_catalog_snapshot(&self, data_root: &Path) -> SpriteCatalogSnapshot {
        let mut image_owners = BTreeMap::new();
        for (owner, image) in &self.sprite_sheet_image_owners {
            if let Some(source) = self
                .sprite_sheet_texture_sources
                .get(owner)
                .and_then(|sources| sources.last())
            {
                image_owners.insert(source.clone(), image.clone());
            }
        }
        for (owner, image) in &self.bitmap_font_image_owners {
            if let Some(source) = self.bitmap_font_texture_sources.get(owner) {
                image_owners.insert(source.clone(), image.clone());
            }
        }
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
                    let Ok(region) = region.snapshot_image() else {
                        continue;
                    };
                    let texture_source = region.texture_source.clone();
                    regions.insert(name.clone(), (*region).clone());
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
                        .filter_map(|(index, (part, region))| {
                            let region = Arc::new(region.clone()).snapshot_image().ok()?;
                            let alias = format!("<composite:{}/{}/{}>", entry.owner, name, index);
                            regions.insert(alias.clone(), region.as_ref().clone());
                            let mut part = part.clone();
                            part.sprite = alias;
                            Some(part)
                        })
                        .collect();
                    composites.insert(name.clone(), bound_parts);
                }
            }
        }
        SpriteCatalogSnapshot {
            image_owners,
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
        let metrics = entry.metrics;
        Some(SpriteGeometry {
            min_x: -f64::from(metrics.pivot_x),
            min_y: -f64::from(metrics.pivot_y),
            max_x: f64::from(metrics.width) - f64::from(metrics.pivot_x),
            max_y: f64::from(metrics.height) - f64::from(metrics.pivot_y),
        })
    }

    pub(crate) fn active_atlas_catalog_region(
        &self,
        name: &str,
        data_root: &Path,
    ) -> Option<Arc<SpriteCatalogRegion>> {
        let asset_name = name.split_once('#').map_or(name, |(base, _)| base);
        let entry = self.active_sprite_entry(asset_name, Some(SpriteResourceKind::Atlas))?;
        entry.atlas_region.clone().or_else(|| {
            // Direct ResourceRuntime fixtures can install parsed values
            // without completing the production SpriteSheet constructor.
            self.sprite_sheet_catalog_region(&entry.owner, asset_name, data_root)
        })
    }

    /// Resolve the concrete AtlasSprite pointer and its already-retained COW
    /// label together. Instance suffixes are not part of the resource lookup,
    /// but remain visible on the deferred diagnostic command just as before.
    pub(crate) fn active_atlas_draw_binding(
        &self,
        name: &str,
        data_root: &Path,
    ) -> Option<(SharedSpriteName, Arc<SpriteCatalogRegion>)> {
        let asset_name = name.split_once('#').map_or(name, |(base, _)| base);
        let entry = self.active_sprite_entry(asset_name, Some(SpriteResourceKind::Atlas))?;
        let region = entry
            .atlas_region
            .clone()
            .or_else(|| self.sprite_sheet_catalog_region(&entry.owner, asset_name, data_root))?;
        let label = if asset_name.len() == name.len() {
            entry.name.clone()
        } else {
            name.into()
        };
        Some((label, region))
    }

    #[cfg(test)]
    pub(crate) fn active_masked_texture_source(
        &self,
        name: &str,
        data_root: &Path,
    ) -> Option<String> {
        self.active_masked_texture_binding(name, data_root)
            .and_then(|binding| binding.source().map(str::to_owned))
    }

    pub(crate) fn active_masked_texture_binding(
        &self,
        name: &str,
        data_root: &Path,
    ) -> Option<MaskedTextureBinding> {
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
        let texture_index = self
            .sprite_sheet_values
            .get(owner)?
            .textures
            .len()
            .checked_sub(1)?;
        let source = self.sprite_sheet_image_source(owner, texture_index, data_root)?;
        let image = self
            .sprite_sheet_decoded_images
            .get(owner)
            .and_then(|images| images.get(texture_index))
            .cloned()
            .flatten();
        let image_owner = self.sprite_sheet_image_owners.get(owner).cloned();
        Some(MaskedTextureBinding::with_image(source, image, image_owner))
    }

    pub(crate) fn borrow_masked_texture_binding(
        &self,
        name: &str,
        data_root: &Path,
    ) -> Option<MaskedTextureBinding> {
        let binding = self.active_masked_texture_binding(name, data_root)?;
        let owner = if self.sprite_sheet_values.contains_key(name) {
            name.to_owned()
        } else {
            crate::resource_manager::resource_double_file_stem(name)
        };
        let Some((cell, image_owner)) = self
            .sprite_sheet_image_cells
            .get(&owner)
            .zip(binding.image_owner())
        else {
            // Immutable diagnostic fixtures have no native resource cell.
            return Some(binding);
        };
        Some(MaskedTextureBinding::Borrowed {
            source: binding.source()?.to_owned(),
            image: crate::NativeImageBorrow::new(cell.bind(), image_owner.identity()),
        })
    }

    fn sprite_sheet_image_source(
        &self,
        owner: &str,
        texture_index: usize,
        data_root: &Path,
    ) -> Option<String> {
        self.sprite_sheet_texture_sources
            .get(owner)
            .and_then(|textures| textures.get(texture_index))
            .cloned()
            .or_else(|| {
                let texture = self
                    .sprite_sheet_values
                    .get(owner)?
                    .textures
                    .get(texture_index)?;
                let descriptor = self.sprite_sheet_descriptor_paths.get(owner);
                let path = resolve_file_image_path(data_root, descriptor, texture);
                let identity = self
                    .sprite_sheet_identities
                    .get(owner)
                    .copied()
                    .unwrap_or(0);
                Some(sheet_image_source(identity, texture_index, &path))
            })
    }

    pub(crate) fn active_composite_parts(&self, name: &str) -> Option<&[CompositePart]> {
        let asset_name = name.split_once('#').map_or(name, |(base, _)| base);
        let entry = self.active_sprite_entry(asset_name, Some(SpriteResourceKind::Composite))?;
        self.composite_set_values
            .get(&entry.owner)?
            .sprites
            .get(entry.index)
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
            .get(entry.index)?
            .parts
            .as_slice();
        let regions = self
            .composite_set_regions
            .get(&entry.owner)?
            .get(entry.index)?
            .as_slice();
        (parts.len() == regions.len()).then_some((parts, regions))
    }

    pub(crate) fn active_bound_composite(&self, name: &str) -> Option<Arc<CompositeSpriteOwner>> {
        let asset_name = name.split_once('#').map_or(name, |(base, _)| base);
        self.active_sprite_entry(asset_name, Some(SpriteResourceKind::Composite))?
            .composite_sprite
            .clone()
    }

    /// Borrow the concrete CompoSprite from its active stack entry and retain
    /// only the immutable child array required past this synchronous lookup.
    /// Direct native draws do not acquire another owner reference before they
    /// walk the CompoSprite Entry vector.
    pub(crate) fn active_bound_composite_snapshot(
        &self,
        name: &str,
    ) -> Option<Arc<Vec<BoundCompositePart>>> {
        let asset_name = name.split_once('#').map_or(name, |(base, _)| base);
        self.active_sprite_entry(asset_name, Some(SpriteResourceKind::Composite))?
            .composite_sprite
            .as_ref()
            .map(|owner| owner.snapshot())
    }

    /// Publish the latest mutable Entry records through the concrete
    /// CompoSprite owner retained by already-created particles and scene
    /// objects. Deferred render commands keep the older Arc snapshot they
    /// captured before this replacement.
    pub(crate) fn refresh_active_bound_composite(
        &self,
        name: &str,
    ) -> Option<Arc<CompositeSpriteOwner>> {
        let asset_name = name.split_once('#').map_or(name, |(base, _)| base);
        let entry = self.active_sprite_entry(asset_name, Some(SpriteResourceKind::Composite))?;
        let owner = Arc::clone(entry.composite_sprite.as_ref()?);
        let parts = self
            .composite_set_values
            .get(&entry.owner)?
            .sprites
            .get(entry.index)?
            .parts
            .as_slice();
        let regions = self
            .composite_set_regions
            .get(&entry.owner)?
            .get(entry.index)?
            .as_slice();
        (parts.len() == regions.len()).then_some(())?;
        owner.replace(
            parts
                .iter()
                .cloned()
                .zip(regions.iter().cloned())
                .map(|(part, region)| BoundCompositePart {
                    sprite: part.sprite.as_str().into(),
                    part,
                    region: Arc::new(region),
                })
                .collect(),
        );
        Some(owner)
    }

    pub(crate) fn active_composite_parts_mut(
        &mut self,
        name: &str,
    ) -> Option<&mut Vec<CompositePart>> {
        let asset_name = name.split_once('#').map_or(name, |(base, _)| base);
        let entry = self.active_sprite_entry(asset_name, Some(SpriteResourceKind::Composite))?;
        let owner = entry.owner.clone();
        let index = entry.index;
        self.composite_set_values
            .get_mut(&owner)?
            .sprites
            .get_mut(index)
            .map(|sprite| &mut sprite.parts)
    }

    /// Match CompoSprite::setEntryName (`sub_1004375E8`): resolve the new
    /// AtlasSprite through the ordered sheet map, replace the retained child
    /// pointer, then immediately rebuild the composite's cached bounds.
    pub(crate) fn rebind_active_composite_part_region(
        &mut self,
        name: &str,
        part_index: usize,
        sprite_name: &str,
    ) -> Option<NativeSpriteMetrics> {
        let atlas_name = sprite_name
            .split_once('#')
            .map_or(sprite_name, |(base, _)| base);
        let region = self
            .sprite_sheet_values
            .keys()
            .find_map(|owner| {
                self.sprite_sheet_catalog_regions
                    .get(owner)
                    .and_then(|regions| regions.get(atlas_name))
            })?
            .as_ref()
            .clone();
        let asset_name = name.split_once('#').map_or(name, |(base, _)| base);
        let entry = self.active_sprite_entry(asset_name, Some(SpriteResourceKind::Composite))?;
        let owner = entry.owner.clone();
        let composite_index = entry.index;
        *self
            .composite_set_regions
            .get_mut(&owner)?
            .get_mut(composite_index)?
            .get_mut(part_index)? = region;
        self.refresh_active_composite_metrics(asset_name)
    }

    pub(crate) fn active_geometry(&self, name: &str) -> Option<SpriteGeometry> {
        self.resolve_active_geometry(name, &mut BTreeSet::new())
    }

    /// Return the concrete integer fields used by Purple's native Sprite
    /// queries (`getBoundsX/Y` and `getPivotX/Y`). Both concrete Sprite types
    /// expose stored members; CompoSprite refreshes those members only at its
    /// native `updateBounds` call sites.
    pub(crate) fn active_native_sprite_metrics(&self, name: &str) -> Option<NativeSpriteMetrics> {
        let asset_name = name.split_once('#').map_or(name, |(base, _)| base);
        self.active_sprite_entry(asset_name, None)
            .map(|entry| entry.metrics)
    }

    /// Rebuild CompoSprite's cached integer fields at the explicit native
    /// `getCompoSpriteBounds` boundary (`sub_10044913C`). Ordinary generic
    /// bounds/pivot queries only read the last values stored on the object.
    pub(crate) fn refresh_active_composite_metrics(
        &mut self,
        name: &str,
    ) -> Option<NativeSpriteMetrics> {
        let asset_name = name.split_once('#').map_or(name, |(base, _)| base);
        let entry = self.sprite_entries.get_mut(asset_name)?.last_mut()?;
        if entry.kind != SpriteResourceKind::Composite {
            return None;
        }
        let owner = &entry.owner;
        let index = entry.index;
        let parts = &self
            .composite_set_values
            .get(owner)?
            .sprites
            .get(index)?
            .parts;
        let regions = self.composite_set_regions.get(owner)?.get(index)?;
        let metrics =
            native_composite_metrics_from_parts(parts, regions).unwrap_or(NativeSpriteMetrics {
                width: 0,
                height: 0,
                pivot_x: 0,
                pivot_y: 0,
            });
        entry.metrics = metrics;
        Some(metrics)
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
