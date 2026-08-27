//! SpriteSheet and CompoSpriteSet construction, replacement and release.

use std::{path::Path, sync::Arc};

use stella_assets::ka3d::{CompositeSpriteSet, SpriteSheet};

use super::{ResourceRuntime, SpriteResourceEntry, SpriteResourceKind};
use crate::{
    BoundCompositePart, CompositeSpriteOwner, SpriteCatalogRegion,
    resource_manager::{NativeSpriteMetrics, native_composite_metrics_from_parts},
};

impl ResourceRuntime {
    pub(crate) fn register_sprite_aliases(&mut self, aliases: &[(&str, &str)]) {
        let mut changed = false;
        for &(alias, target) in aliases {
            if self
                .sprite_aliases
                .insert(alias.to_owned(), target.to_owned())
                .as_deref()
                != Some(target)
            {
                changed = true;
            }
        }
        if !changed {
            return;
        }

        // Platform services are normally installed before the first script
        // sheet is loaded. Keep the operation correct for tests and late
        // service activation too by materialising aliases in existing sheets.
        let aliases = self.sprite_aliases.clone();
        let mut additions = Vec::new();
        for (owner, sheet) in &mut self.sprite_sheet_values {
            let existing = sheet
                .sprites
                .iter()
                .map(|sprite| sprite.name.clone())
                .collect::<std::collections::BTreeSet<_>>();
            for (alias, target) in &aliases {
                if existing.contains(alias) {
                    continue;
                }
                let Some(target_index) = sheet
                    .sprites
                    .iter()
                    .position(|sprite| sprite.name == *target)
                else {
                    continue;
                };
                let mut sprite = sheet.sprites[target_index].clone();
                let texture_index = sheet
                    .sprite_texture_indices
                    .get(target_index)
                    .copied()
                    .unwrap_or(0);
                sprite.name = alias.clone();
                let index = sheet.sprites.len();
                let metrics = NativeSpriteMetrics {
                    width: i32::from(sprite.width),
                    height: i32::from(sprite.height),
                    pivot_x: i32::from(sprite.pivot_x),
                    pivot_y: i32::from(sprite.pivot_y),
                };
                sheet.sprites.push(sprite);
                sheet.sprite_texture_indices.push(texture_index);
                additions.push((alias.clone(), owner.clone(), index, metrics));
            }
        }
        for (alias, owner, index, metrics) in additions {
            self.sprite_entries
                .entry(alias)
                .or_default()
                .push(SpriteResourceEntry {
                    kind: SpriteResourceKind::Atlas,
                    owner: owner.clone(),
                    index,
                    metrics,
                    atlas_region: None,
                    composite_sprite: None,
                });
            self.sprite_sheet_catalog_regions.remove(&owner);
        }
        self.mark_sprite_catalog_changed();
    }

    pub(crate) fn replace_sprite_sheet_value(&mut self, owner: &str, mut sheet: SpriteSheet) {
        let existing = sheet
            .sprites
            .iter()
            .map(|sprite| sprite.name.clone())
            .collect::<std::collections::BTreeSet<_>>();
        for (alias, target) in &self.sprite_aliases {
            if existing.contains(alias) {
                continue;
            }
            let Some(target_index) = sheet
                .sprites
                .iter()
                .position(|sprite| sprite.name == *target)
            else {
                continue;
            };
            let mut sprite = sheet.sprites[target_index].clone();
            let texture_index = sheet
                .sprite_texture_indices
                .get(target_index)
                .copied()
                .unwrap_or(0);
            sprite.name = alias.clone();
            sheet.sprites.push(sprite);
            sheet.sprite_texture_indices.push(texture_index);
        }
        self.sprite_sheet_texture_sources.remove(owner);
        self.sprite_sheet_catalog_regions.remove(owner);
        if let Some(old) = self.sprite_sheet_values.remove(owner) {
            let names = old
                .sprites
                .into_iter()
                .map(|sprite| sprite.name)
                .collect::<Vec<_>>();
            self.remove_sprite_entries(SpriteResourceKind::Atlas, owner, &names);
        }
        let native_sheet_id = self.next_sprite_sheet_identity;
        self.next_sprite_sheet_identity = self.next_sprite_sheet_identity.wrapping_add(1).max(1);
        self.sprite_sheet_identities
            .insert(owner.to_owned(), native_sheet_id);
        for (index, sprite) in sheet.sprites.iter().enumerate() {
            self.sprite_entries
                .entry(sprite.name.clone())
                .or_default()
                .push(SpriteResourceEntry {
                    kind: SpriteResourceKind::Atlas,
                    owner: owner.to_owned(),
                    index,
                    metrics: NativeSpriteMetrics {
                        width: i32::from(sprite.width),
                        height: i32::from(sprite.height),
                        pivot_x: i32::from(sprite.pivot_x),
                        pivot_y: i32::from(sprite.pivot_y),
                    },
                    atlas_region: None,
                    composite_sprite: None,
                });
        }
        self.sprite_sheet_values.insert(owner.to_owned(), sheet);
        self.mark_sprite_catalog_changed();
    }

    pub(crate) fn remove_sprite_sheet_value(&mut self, owner: &str) {
        self.sprite_sheet_identities.remove(owner);
        self.sprite_sheet_texture_sources.remove(owner);
        self.sprite_sheet_catalog_regions.remove(owner);
        if let Some(sheet) = self.sprite_sheet_values.remove(owner) {
            let names = sheet
                .sprites
                .into_iter()
                .map(|sprite| sprite.name)
                .collect::<Vec<_>>();
            self.remove_sprite_entries(SpriteResourceKind::Atlas, owner, &names);
            self.mark_sprite_catalog_changed();
        }
    }

    /// Remove a retained sheet's entries from `Resources + 0x588` without
    /// destroying the sheet map value. This is the two-stage
    /// `sub_1004578BC` + `sub_10046AF40` path selected by
    /// `releaseSpriteSheet(path, true)`.
    pub(crate) fn deactivate_sprite_sheet_value(&mut self, owner: &str) {
        self.sprite_sheet_texture_sources.remove(owner);
        self.sprite_sheet_catalog_regions.remove(owner);
        if let Some(sheet) = self.sprite_sheet_values.get(owner) {
            let names = sheet
                .sprites
                .iter()
                .map(|sprite| sprite.name.clone())
                .collect::<Vec<_>>();
            self.remove_sprite_entries(SpriteResourceKind::Atlas, owner, &names);
            self.mark_sprite_catalog_changed();
        }
    }

    pub(crate) fn replace_composite_set_value(
        &mut self,
        owner: &str,
        set: CompositeSpriteSet,
        regions: Vec<Vec<SpriteCatalogRegion>>,
    ) {
        if let Some(old) = self.composite_set_values.remove(owner) {
            let names = old
                .sprites
                .into_iter()
                .map(|sprite| sprite.name)
                .collect::<Vec<_>>();
            self.remove_sprite_entries(SpriteResourceKind::Composite, owner, &names);
        }
        for (index, sprite) in set.sprites.iter().enumerate() {
            let part_regions = regions.get(index).map(Vec::as_slice).unwrap_or_default();
            debug_assert_eq!(sprite.parts.len(), part_regions.len());
            let bound_parts = sprite
                .parts
                .iter()
                .cloned()
                .zip(part_regions.iter().cloned())
                .map(|(part, region)| BoundCompositePart { part, region })
                .collect();
            let metrics = regions
                .get(index)
                .and_then(|regions| native_composite_metrics_from_parts(&sprite.parts, regions))
                .unwrap_or(NativeSpriteMetrics {
                    width: 0,
                    height: 0,
                    pivot_x: 0,
                    pivot_y: 0,
                });
            self.sprite_entries
                .entry(sprite.name.clone())
                .or_default()
                .push(SpriteResourceEntry {
                    kind: SpriteResourceKind::Composite,
                    owner: owner.to_owned(),
                    index,
                    metrics,
                    atlas_region: None,
                    composite_sprite: Some(Arc::new(CompositeSpriteOwner::new(bound_parts))),
                });
        }
        self.composite_set_values.insert(owner.to_owned(), set);
        self.composite_set_regions.insert(owner.to_owned(), regions);
        self.mark_sprite_catalog_changed();
    }

    pub(crate) fn remove_composite_set_value(&mut self, owner: &str) {
        self.composite_set_regions.remove(owner);
        if let Some(set) = self.composite_set_values.remove(owner) {
            let names = set
                .sprites
                .into_iter()
                .map(|sprite| sprite.name)
                .collect::<Vec<_>>();
            self.remove_sprite_entries(SpriteResourceKind::Composite, owner, &names);
            self.mark_sprite_catalog_changed();
        }
    }

    pub(crate) fn mark_sprite_catalog_changed(&mut self) {
        self.sprite_catalog_revision = self.sprite_catalog_revision.wrapping_add(1).max(1);
    }

    /// Resolve COMP children exactly once through the ordered SpriteSheet
    /// map, as `sub_100461B98` does before constructing each native part.
    pub(crate) fn bind_composite_set_regions(
        &self,
        set: &CompositeSpriteSet,
        data_root: &Path,
        source: &str,
    ) -> Result<Vec<Vec<SpriteCatalogRegion>>, String> {
        let mut result = Vec::with_capacity(set.sprites.len());
        for composite in &set.sprites {
            let mut regions = Vec::with_capacity(composite.parts.len());
            for part in &composite.parts {
                let sprite_name = part
                    .sprite
                    .split_once('#')
                    .map_or(part.sprite.as_str(), |(base, _)| base);
                let region = self
                    .sprite_sheet_values
                    .iter()
                    .find_map(|(owner, _)| {
                        self.sprite_sheet_catalog_region(owner, sprite_name, data_root)
                    })
                    .ok_or_else(|| {
                        format!(
                            "Sprite \"{}\" not loaded while loading {}",
                            part.sprite, source
                        )
                    })?;
                regions.push((*region).clone());
            }
            result.push(regions);
        }
        Ok(result)
    }

    fn remove_sprite_entries(&mut self, kind: SpriteResourceKind, owner: &str, names: &[String]) {
        let mut empty = Vec::new();
        for name in names {
            if let Some(entries) = self.sprite_entries.get_mut(name) {
                entries.retain(|entry| entry.kind != kind || entry.owner != owner);
                if entries.is_empty() {
                    empty.push(name.clone());
                }
            }
        }
        for name in empty {
            self.sprite_entries.remove(&name);
        }
    }
}
