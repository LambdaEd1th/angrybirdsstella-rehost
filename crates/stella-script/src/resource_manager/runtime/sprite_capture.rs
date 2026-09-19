//! Resources::captureSprite (0x100458F54) and retained Image lifetime.

use std::path::Path;

use stella_assets::ka3d::{SpriteRegion, SpriteSheet};

use super::ResourceRuntime;
use crate::{LuaResult, runtime_error};

impl ResourceRuntime {
    pub(crate) fn capture_sprite(
        &mut self,
        name: &str,
        dimensions: [u32; 2],
        data_root: &Path,
    ) -> LuaResult<(String, bool)> {
        // Unlike create/releaseSpriteSheet, this entry uses the exact string
        // as the sheet-map key. Existing values retain their geometry and
        // active-name stack position, even when another sheet shadows them.
        if self.sprite_sheets.contains(name) {
            if !self.released_sprite_sheet_resources.contains(name)
                && let Some(texture_index) = self
                    .sprite_sheet_values
                    .get(name)
                    .and_then(|sheet| sheet.textures.len().checked_sub(1))
            {
                let retained_dimensions =
                    self.sprite_sheet_image_dimensions
                        .get(name)
                        .ok_or_else(|| {
                            runtime_error(format!("Capture target image unavailable: {name}"))
                        })?;
                // GL_Context::capture at 0x10059A2BC throws before modifying
                // the image. Resizing the window cannot resize old captures.
                if retained_dimensions != &dimensions {
                    return Err(runtime_error("Wrong size capture target image"));
                }
                return self
                    .sprite_sheet_texture_sources
                    .get(name)
                    .and_then(|sources| sources.get(texture_index))
                    .cloned()
                    .map(|source| (source, false))
                    .ok_or_else(|| {
                        runtime_error(format!("Capture target image unavailable: {name}"))
                    });
            }
            // A retained sheet with a null Image (release(..., true)) takes
            // the existing-map path: capture allocates a temporary image,
            // then drops it. It does not republish sprites or restore +0x20.
            return Ok((self.allocate_capture_image_source(), true));
        }

        let texture_source = self.allocate_capture_image_source();
        self.replace_sprite_sheet_value(
            name,
            SpriteSheet {
                textures: vec![texture_source.clone()],
                sprites: vec![SpriteRegion {
                    name: name.to_owned(),
                    x: 0,
                    y: 0,
                    width: dimensions[0] as i16,
                    height: dimensions[1] as i16,
                    pivot_x: 0,
                    pivot_y: 0,
                    // New GL_Image's flipped flag is true (0x10059A200).
                    // Resources compensates bottom-up capture using flags 3.
                    atlas_rotation: 3,
                }],
                sprite_texture_indices: vec![0],
            },
        );
        self.sprite_sheets.insert(name.to_owned());
        self.sprite_sheet_image_dimensions
            .insert(name.to_owned(), dimensions);
        self.cache_sprite_sheet_host_bindings(name, data_root);
        Ok((texture_source, false))
    }

    fn allocate_capture_image_source(&mut self) -> String {
        let identity = self.next_capture_image_identity;
        self.next_capture_image_identity = identity.wrapping_add(1).max(1);
        format!("<capture:image:{identity}>")
    }
}
