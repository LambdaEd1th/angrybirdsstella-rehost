//! SpriteSheet and CompoSpriteSet construction, replacement and release.

use std::{collections::BTreeMap, path::Path};

use stella_assets::ka3d::{CompositeSpriteSet, SpriteSheet};

use super::{ResourceRuntime, SpriteResourceEntry, SpriteResourceKind};
use crate::SpriteCatalogRegion;

impl ResourceRuntime {
    pub(crate) fn replace_sprite_sheet_value(&mut self, owner: &str, sheet: SpriteSheet) {
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
        for sprite in &sheet.sprites {
            self.sprite_entries
                .entry(sprite.name.clone())
                .or_default()
                .push(SpriteResourceEntry {
                    kind: SpriteResourceKind::Atlas,
                    owner: owner.to_owned(),
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
        regions: BTreeMap<String, Vec<SpriteCatalogRegion>>,
    ) {
        if let Some(old) = self.composite_set_values.remove(owner) {
            let names = old
                .sprites
                .into_iter()
                .map(|sprite| sprite.name)
                .collect::<Vec<_>>();
            self.remove_sprite_entries(SpriteResourceKind::Composite, owner, &names);
        }
        for sprite in &set.sprites {
            self.sprite_entries
                .entry(sprite.name.clone())
                .or_default()
                .push(SpriteResourceEntry {
                    kind: SpriteResourceKind::Composite,
                    owner: owner.to_owned(),
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
    ) -> Result<BTreeMap<String, Vec<SpriteCatalogRegion>>, String> {
        let mut result = BTreeMap::new();
        for composite in &set.sprites {
            let mut regions = Vec::with_capacity(composite.parts.len());
            for part in &composite.parts {
                let region = self
                    .sprite_sheet_values
                    .iter()
                    .find_map(|(owner, _)| {
                        self.sprite_sheet_catalog_region(owner, &part.sprite, data_root)
                    })
                    .ok_or_else(|| {
                        format!(
                            "Sprite \"{}\" not loaded while loading {}",
                            part.sprite, source
                        )
                    })?;
                regions.push(region);
            }
            result.insert(composite.name.clone(), regions);
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
