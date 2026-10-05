//! TexturizedSprite's raw Image pair and three geometry vectors.

use crate::*;

#[derive(Debug, Default)]
pub(crate) struct NativeMaskedBatch {
    commands: Vec<RenderCommand>,
    fill: Option<MaskedTextureBinding>,
    mask: Option<MaskedTextureBinding>,
}

impl RenderBridge {
    pub(crate) fn ensure_named_masked_batch(&mut self, name: Arc<str>) {
        self.named_masked_batches.entry(name).or_default();
    }

    pub(crate) fn push_named_masked_command(
        &mut self,
        mut command: RenderCommand,
        position: [f32; 2],
        texture_scales: [f32; 2],
    ) -> LuaResult<()> {
        let texture = command
            .texture
            .as_ref()
            .expect("named mask requires a fill");
        self.ensure_named_masked_batch(Arc::clone(&texture.name));
        let Some(region) = command.bound_region.as_ref() else {
            // Existing immutable diagnostics can have no native AtlasSprite.
            return self.push_render_command(command);
        };
        let (positions, rotated, origin) = masked_positions(
            &region.sprite,
            self.state,
            position,
            [self.screen_width, self.screen_height],
        );
        // sub_10008D428 tests raw NDC bounds before touching either Image.
        let [minimum_x, minimum_y, maximum_x, maximum_y] = positions.iter().fold(
            [f32::MAX, f32::MAX, f32::MIN, f32::MIN],
            |[min_x, min_y, max_x, max_y], [x, y]| {
                [min_x.min(*x), min_y.min(*y), max_x.max(*x), max_y.max(*y)]
            },
        );
        if !(maximum_x >= -1.0 && maximum_y >= -1.0 && minimum_x < 1.0 && minimum_y < 1.0) {
            return Ok(());
        }
        if matches!(texture.binding, MaskedTextureBinding::Missing) {
            return self.push_render_command(command);
        }
        let (mask, mask_dimensions) = region.borrow_image()?;
        // Valid native cells always carry their constructor Image extents.
        // Source-only immutable diagnostics keep the existing unit-image path.
        let mask_dimensions = mask_dimensions.unwrap_or([1, 1]);
        let fill_dimensions = texture.binding.native_dimensions()?.unwrap_or([1, 1]);
        // GL_Image stores the same W/H passed to GL_Texture (59E06C,
        // 5A0614); NPOT changes wrapping/mips, never allocation extent.
        let inverse_fill: [f32; 2] = std::array::from_fn(|axis| {
            let image_extent = fill_dimensions[axis] as f32;
            let divided_extent = image_extent / texture_scales[axis];
            let image_to_texture = image_extent / fill_dimensions[axis] as f32;
            1.0_f32 / (divided_extent * image_to_texture)
        });
        let quad = NativeMaskedQuad {
            positions,
            mask_uv: region
                .sprite
                .native_mask_uvs(mask_dimensions[0] as f32, mask_dimensions[1] as f32),
            fill_uv: rotated.map(|point| {
                std::array::from_fn(|axis| (origin[axis] + point[axis]) * inverse_fill[axis])
            }),
        };
        let key = Arc::clone(&texture.name);
        let fill = texture.binding.clone();
        command.texture = Some(Arc::new(SpriteTextureSubmission {
            native_quad: Some(Arc::new(quad)),
            ..texture.as_ref().clone()
        }));
        // The native vectors own floats, not AtlasSprite/Sheet/Image owners.
        // Keep only detached diagnostic metadata beside the geometry.
        command.bound_region = Some(Arc::new(SpriteCatalogRegion {
            sheet_image: None,
            image_owner: None,
            decoded_image: None,
            ..region.as_ref().clone()
        }));
        command.shader = None;
        let batch = self
            .named_masked_batches
            .get_mut(&key)
            .expect("named mask exists");
        batch.fill = Some(fill);
        batch.mask = Some(mask);
        batch.commands.push(command);
        Ok(())
    }

    pub(crate) fn flush_named_masked_batches(&mut self) -> LuaResult<()> {
        let projection = self.projection_3d().map(Arc::new);
        let alpha = self.state.alpha as f32;
        let clip_rect = self.state.clip_rect;
        // std::map<string, Ptr<TexturizedSprite>> visits names by byte order.
        for batch in self.named_masked_batches.values_mut() {
            if batch.commands.is_empty() {
                continue;
            }
            // 08DBC0 dereferences the last raw fill, then the last raw mask.
            // Freeze those Images only at this actual draw boundary.
            let fill = batch
                .fill
                .as_ref()
                .expect("nonempty mask has a fill")
                .snapshot()?;
            let mask = batch
                .mask
                .as_ref()
                .expect("nonempty mask has an Image")
                .snapshot()?;
            for mut command in batch.commands.drain(..) {
                let texture = Arc::make_mut(command.texture.as_mut().expect("masked command"));
                texture.binding = fill.clone();
                let region = Arc::make_mut(command.bound_region.as_mut().expect("masked region"));
                region.texture_source =
                    mask.source().expect("visible mask has a source").to_owned();
                region.image_owner = mask.image_owner().cloned();
                region.decoded_image = mask.image().cloned();
                command.state.alpha = alpha;
                command.state.clip_rect = clip_rect;
                command.projection_3d = projection.clone();
                command.order = self.next_draw_order;
                self.next_draw_order = self.next_draw_order.wrapping_add(1);
                self.commands.push(command);
            }
            // Native flush clears lengths/pointers but keeps vector capacity.
            batch.fill = None;
            batch.mask = None;
        }
        Ok(())
    }
}

fn masked_positions(
    region: &stella_assets::ka3d::SpriteRegion,
    state: RenderState,
    position: [f32; 2],
    dimensions: [u32; 2],
) -> ([[f32; 2]; 4], [[f32; 2]; 4], [f32; 2]) {
    let atlas_pivot = [f32::from(region.pivot_x), f32::from(region.pivot_y)];
    let relative = [position[0] - atlas_pivot[0], position[1] - atlas_pivot[1]];
    let pivot = [state.pivot_x as f32, state.pivot_y as f32];
    let (sine, cosine) = (state.angle as f32).sin_cos();
    let [width, height] = [f32::from(region.width), f32::from(region.height)];
    // 08D428 uses FMUL/FADD here, unlike ordinary Sprite's fused dot product.
    let left_x = cosine * -pivot[0];
    let top_x = -sine * -pivot[1];
    let right_x = cosine * (width - pivot[0]);
    let bottom_x = -sine * (height - pivot[1]);
    let left_y = sine * -pivot[0];
    let top_y = cosine * -pivot[1];
    let right_y = sine * (width - pivot[0]);
    let bottom_y = cosine * (height - pivot[1]);
    let rotated = [
        [left_x + top_x, left_y + top_y],
        [top_x + right_x, top_y + right_y],
        [left_x + bottom_x, left_y + bottom_y],
        [right_x + bottom_x, right_y + bottom_y],
    ];
    let offset = [
        state.translate_x as f32 + (relative[0] + pivot[0]),
        state.translate_y as f32 + (relative[1] + pivot[1]),
    ];
    let scale_x = state.scale_x as f32;
    let scale_y = state.scale_y as f32;
    let viewport = [
        (scale_x + scale_x) / dimensions[0] as f32,
        (scale_y * -2.0_f32) / dimensions[1] as f32,
    ];
    let positions = rotated.map(|[x, y]| {
        [
            viewport[0].mul_add(offset[0] + x, -1.0),
            viewport[1].mul_add(offset[1] + y, 1.0),
        ]
    });
    let origin = [relative[0] + atlas_pivot[0], relative[1] + atlas_pivot[1]];
    (positions, rotated, origin)
}
