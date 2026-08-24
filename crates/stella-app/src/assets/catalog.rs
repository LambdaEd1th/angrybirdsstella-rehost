use super::*;

impl AssetCatalog {
    pub(crate) fn load(root: PathBuf, font_root: PathBuf) -> Result<Self> {
        let mut entries = fs::read_dir(&root)
            .with_context(|| format!("read asset directory {}", root.display()))?
            .collect::<std::io::Result<Vec<_>>>()?;
        entries.sort_by_key(|entry| entry.file_name());
        let mut regions = HashMap::new();
        let mut composites = HashMap::new();
        let mut masked_textures = HashMap::new();
        for entry in entries {
            let path = entry.path();
            if path.extension().and_then(|value| value.to_str()) != Some("dat") {
                continue;
            }
            let bytes = fs::read(&path).with_context(|| format!("read {}", path.display()))?;
            if Ka3dEnvelope::find(&bytes, b"SPRT").is_ok() {
                let sheet = SpriteSheet::parse(&bytes)
                    .with_context(|| format!("parse sprite sheet {}", path.display()))?;
                if sheet.sprites.len() == 1
                    && let (Some(name), Some(texture)) = (
                        path.file_stem().and_then(|value| value.to_str()),
                        sheet.texture_for(&sheet.sprites[0]),
                    )
                {
                    masked_textures.insert(name.to_owned(), texture.to_owned());
                }
                for (index, sprite) in sheet.sprites.into_iter().enumerate() {
                    let Some(texture) = sheet
                        .sprite_texture_indices
                        .get(index)
                        .and_then(|index| sheet.textures.get(*index))
                        .cloned()
                    else {
                        continue;
                    };
                    regions.insert(sprite.name.clone(), AtlasRegion { texture, sprite });
                }
            } else if Ka3dEnvelope::find(&bytes, b"COMP").is_ok() {
                let set = CompositeSpriteSet::parse(&bytes)
                    .with_context(|| format!("parse composite set {}", path.display()))?;
                for sprite in set.sprites {
                    composites.insert(sprite.name, sprite.parts);
                }
            }
        }
        let mut fonts = HashMap::new();
        if let Ok(entries) = fs::read_dir(&font_root) {
            for entry in entries.filter_map(Result::ok) {
                let path = entry.path();
                if path.extension().and_then(|value| value.to_str()) != Some("dat") {
                    continue;
                }
                let Ok(bytes) = fs::read(&path) else {
                    continue;
                };
                let Ok(font) = BitmapFont::parse(&bytes) else {
                    continue;
                };
                if let Some(name) = path.file_stem().and_then(|value| value.to_str()) {
                    fonts.insert(name.to_owned(), font);
                }
            }
        }
        Ok(Self {
            root,
            font_root,
            regions,
            composites,
            masked_textures,
            fonts,
            textures: HashMap::new(),
            system_labels: SystemLabelPool::default(),
        })
    }

    pub(crate) fn apply_sprite_catalog_snapshot(
        &mut self,
        snapshot: SpriteCatalogSnapshot,
    ) -> Result<()> {
        self.regions = snapshot
            .regions
            .into_iter()
            .map(|(name, region)| {
                (
                    name,
                    AtlasRegion {
                        texture: region.texture_source,
                        sprite: region.sprite,
                    },
                )
            })
            .collect();
        self.composites = snapshot.composites.into_iter().collect();
        self.masked_textures = snapshot.masked_textures.into_iter().collect();
        // ResourceManager constructs the GL textures while a SpriteSheet
        // group is loaded.  Deferring PVR decode until a sprite's first draw
        // moved that work into live gameplay and produced one-frame stalls on
        // object-heavy levels. Decode each newly active source at the catalog
        // revision boundary; the renderer will upload it on the covered
        // transition frame that first requires it.
        let mut active_textures = self
            .regions
            .values()
            .map(|region| region.texture.clone())
            .chain(self.masked_textures.values().cloned())
            .collect::<Vec<_>>();
        active_textures.sort_unstable();
        active_textures.dedup();
        for texture in active_textures {
            self.texture(&texture)?;
        }
        Ok(())
    }

    pub(crate) fn apply_composite_updates(
        &mut self,
        updates: std::collections::BTreeMap<String, Vec<CompositePart>>,
    ) {
        self.composites.extend(updates);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use std::path::Path;

    #[test]
    fn active_catalog_revision_decodes_its_texture_before_first_draw() {
        let data_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../build/extracted/data");
        let image_root = data_root.join("images/1024x768");
        let font_root = data_root.join("fonts/1024x768");
        if !image_root.is_dir() || !font_root.is_dir() {
            return;
        }
        let mut catalog = AssetCatalog::load(image_root, font_root).unwrap();
        let (name, region) = catalog
            .regions
            .iter()
            .next()
            .map(|(name, region)| (name.clone(), region.clone()))
            .unwrap();
        let texture = region.texture.clone();
        catalog.textures.clear();
        catalog
            .apply_sprite_catalog_snapshot(SpriteCatalogSnapshot {
                revision: 1,
                regions: BTreeMap::from([(
                    name,
                    SpriteCatalogRegion {
                        native_sheet_id: 1,
                        texture_source: texture.clone(),
                        sprite: region.sprite,
                    },
                )]),
                composites: BTreeMap::new(),
                masked_textures: BTreeMap::new(),
            })
            .unwrap();

        assert!(catalog.textures.contains_key(&texture));
    }

    #[test]
    fn shipped_atlas_and_bitmap_font_regions_resolve_inside_their_textures() {
        let data_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../build/extracted/data");
        let image_root = data_root.join("images/1024x768");
        let font_root = data_root.join("fonts/1024x768");
        if !image_root.is_dir() || !font_root.is_dir() {
            return;
        }
        let mut catalog = AssetCatalog::load(image_root, font_root).unwrap();
        let regions = catalog.regions.values().cloned().collect::<Vec<_>>();
        assert!(!regions.is_empty());
        for region in regions {
            let texture = catalog
                .texture(&region.texture)
                .unwrap_or_else(|error| panic!("{}: {error}", region.sprite.name));
            let width = texture.width() as f32;
            let height = texture.height() as f32;
            for [x, y] in region.sprite.native_atlas_corners() {
                assert!(
                    (0.0..=width).contains(&x) && (0.0..=height).contains(&y),
                    "{} corner ({x},{y}) is outside {} {}x{}",
                    region.sprite.name,
                    region.texture,
                    width,
                    height
                );
            }
        }

        let fonts = catalog.fonts.values().cloned().collect::<Vec<_>>();
        assert!(!fonts.is_empty());
        for font in fonts {
            let texture = catalog
                .texture(&font.texture)
                .unwrap_or_else(|error| panic!("bitmap font texture {}: {error}", font.texture));
            let width = i32::try_from(texture.width()).unwrap();
            let height = i32::try_from(texture.height()).unwrap();
            for glyph in &font.glyphs {
                let left = i32::from(glyph.x);
                let top = i32::from(glyph.y);
                let right = left + i32::from(glyph.width);
                let bottom = top + i32::from(glyph.height);
                assert!(
                    left >= 0 && top >= 0 && right <= width && bottom <= height,
                    "font glyph {} region ({left},{top})-({right},{bottom}) is outside {} {}x{}",
                    glyph.codepoint,
                    font.texture,
                    width,
                    height
                );
            }
        }
    }
}
