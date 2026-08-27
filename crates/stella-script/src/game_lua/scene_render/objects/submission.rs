//! Ordinary sprite, ray and decoration submission owned by `sub_10006D5B4`.

use super::pivot::native_scene_callback_pivot;
use super::{SceneCallbackObject, SceneDrawObject};
use crate::*;

impl RenderBridge {
    /// Submit the rectangular-water branch at `0x10004BD34..0x10004BF20`.
    /// Purple draws the interpolated fixture rectangle before consulting the
    /// Lua pre/post callbacks, then suppresses its `RED_CROSS` editor sprite
    /// unless GameLua+0x512 (`setEditing`) is set.
    pub(crate) fn push_scene_water(&mut self, object: &SceneCallbackObject) -> bool {
        if !object.is_water || object.collision_is_circle {
            return false;
        }

        // 0x10004BDA4..0x10004BE88 constructs and memcpy's a complete
        // default GL context before the rectangle draw. It remains current
        // when the editor branch continues into pre-draw, and after a normal
        // water placeholder is suppressed.
        self.state = RenderState::default();

        let pixels_per_physics = 1.0_f32 / f32::from_bits(0x3D4C_CCCD);
        let width = object.native_shape_width as f32;
        let height = object.native_shape_height as f32;
        let half_width = width * 0.5_f32;
        let half_height = height * 0.5_f32;
        let x = object.x as f32;
        let y = object.y as f32;
        let right = x + half_width;
        let bottom = y + half_height;
        let left = x - half_width;
        let top = y - half_height;
        let top_left_x = self.top_left_x as f32;
        let top_left_y = self.top_left_y as f32;
        let world_scale = self.world_scale as f32;

        // The native call supplies the right/bottom corner plus negative
        // width/height to GL_Context::drawRect. Preserve its independent
        // float32 FCVTZS stages before normalizing the deferred quad bounds.
        let screen_right = world_scale * -(top_left_x - pixels_per_physics * right);
        let screen_bottom = world_scale * -(top_left_y - pixels_per_physics * bottom);
        let screen_width = world_scale * (pixels_per_physics * (left - right));
        let screen_height = world_scale * (pixels_per_physics * (top - bottom));
        let screen_right = native_fcvtzs_f32(screen_right);
        let screen_bottom = native_fcvtzs_f32(screen_bottom);
        let screen_width = native_fcvtzs_f32(screen_width);
        let screen_height = native_fcvtzs_f32(screen_height);
        let other_x = (screen_right as f32) + (screen_width as f32);
        let other_y = (screen_bottom as f32) + (screen_height as f32);

        let command = native_rect_command(
            self.water_color,
            f64::from(screen_right),
            f64::from(screen_bottom),
            f64::from(other_x),
            f64::from(other_y),
            RenderState::default(),
        );
        // The 0x9c-byte default-state memcpy clears the prior scissor too.
        self.push_unclipped_rect_command(command);
        true
    }

    pub(crate) fn native_scene_z_bounds(&self) -> (i32, i32) {
        let minimum = self.z_order_min as i32;
        let maximum = if self.z_order_max < 0.0 {
            170
        } else {
            self.z_order_max as i32
        };
        (minimum, maximum)
    }

    #[cfg(test)]
    pub(crate) fn scene_range_entries(&self) -> Vec<(i32, String)> {
        // sub_10004BAB4 uses the stored minimum inclusively and the maximum
        // exclusively. A negative maximum selects its hard-coded 170
        // sentinel for this draw without rewriting GameLua+0x62C.
        let (minimum, maximum) = self.native_scene_z_bounds();
        // Constructors sub_100034740/sub_100034FB0/sub_1000357A4/
        // sub_1000364E0/sub_100036D38 truncate z to an integer, index
        // GameLua+0x310 by that bucket, then index the nested tree by the
        // retained SpriteSheet*. Each leaf is a vector of names appended in
        // render-bucket insertion order. sub_10004BAB4 walks those three
        // levels directly; alphabetical object order is never involved.
        self.scene_render_index.entries_in_range(minimum, maximum)
    }

    #[cfg(test)]
    pub(crate) fn scene_range_names(&self) -> Vec<String> {
        self.scene_range_entries()
            .into_iter()
            .map(|(_, name)| name)
            .collect()
    }

    pub(crate) fn scene_draw_object(&self, name: &str) -> Option<SceneDrawObject> {
        self.scene.get(name).map(SceneDrawObject::from)
    }

    #[cfg(test)]
    pub(crate) fn scene_object_command(&self, object: &SceneDrawObject) -> Option<RenderCommand> {
        (!object.flash_animation
            && object.ray.is_none()
            && !object.sprite.is_empty()
            && object.sprite_bound)
            .then(|| RenderCommand {
                order: 0,
                sprite: object.sprite.clone().into(),
                texture: object.texture.clone(),
                // Purple passes the retained RenderObjectData resource
                // pointers at +0x90/+0x78 into its immediate draw member.
                // Keep both immutable owners shared across the deferred wgpu
                // boundary instead of materializing either resource graph.
                bound_region: object.sprite_region.clone(),
                bound_composite: object.composite_sprite.clone(),
                geometry: None,
                shader: None,
                dirt: object.dirt.as_deref().map(DirtComponent::render_command),
                x: 0.0,
                y: 0.0,
                state: self.scene_object_state(object).into(),
                world_space: true,
            })
    }

    #[cfg(test)]
    pub(crate) fn draw_scene_range(&mut self) {
        let objects = self
            .scene_range_names()
            .into_iter()
            .filter_map(|name| {
                self.scene
                    .get(&name)
                    .filter(|object| object.visible)
                    .map(SceneDrawObject::from)
            })
            .collect::<Vec<_>>();
        let commands = objects
            .iter()
            .filter_map(|object| self.scene_object_command(object))
            .collect::<Vec<_>>();
        self.extend_render_commands(commands);
    }

    pub(crate) fn push_scene_object(
        &mut self,
        object: SceneDrawObject,
        decoration_resources: Option<(&ResourceRuntime, &Path)>,
        shader: Option<SpriteShader>,
    ) {
        // sub_10004BFE0 installs the base object context and then calls the
        // ordinary member with the same GL_Context pointer. The member mutates
        // that context in place before returning to the post callback. Keep
        // both halves in this one bridge acquisition: the previous host split
        // them across two mutex scopes even though no Lua code runs between
        // the live `shader` raw lookup and this submission.
        let command_state = (!object.flash_animation
            && object.ray.is_none()
            && !object.sprite.is_empty()
            && object.sprite_bound)
            .then(|| self.scene_object_state(&object).into());
        self.install_scene_post_draw_state(&object);
        if let Some(ray) = object.ray.as_ref() {
            // DrawablePolygon was constructed as alpha-filled (`mode=1`) and
            // with its optional outline byte clear. Its copied position is
            // projected through the current object callback context.
            for command in
                native_filled_polygon_commands(&ray.vertices, ray.x, ray.y, ray.color, self.state)
            {
                self.push_rect_command(command);
            }
            return;
        }
        if !object.flash_animation
            && object.ray.is_none()
            && !object.sprite.is_empty()
            && object.sprite_bound
        {
            // sub_10006D5B4 looks up the live Lua object's `shader` field at
            // 0x10006D6D8..0x10006D744 immediately before submitting either
            // its ordinary sprite or every composite part. The shader is not
            // a RenderObjectData member and therefore must be supplied from
            // the dispatcher on every draw.
            let dirt = object.dirt.as_deref().map(DirtComponent::render_command);
            let command = RenderCommand {
                order: 0,
                // The compact draw snapshot already retained every native
                // resource pointer while the scene lock was released for Lua.
                // Transfer those owners into the deferred wgpu command rather
                // than retaining and releasing the same pointers a second
                // time. Purple likewise passes +0x90/+0x78 straight through.
                sprite: object.sprite.into(),
                texture: object.texture,
                bound_region: object.sprite_region,
                bound_composite: object.composite_sprite,
                geometry: None,
                shader: shader.map(Arc::new),
                dirt,
                x: 0.0,
                y: 0.0,
                state: command_state.expect("drawable scene object must retain its draw state"),
                world_space: true,
            };
            self.push_render_command(command);
        }
        if object.flash_animation {
            return;
        }
        let Some(decoration) = object.decoration.as_deref() else {
            return;
        };
        if decoration.sprite.is_empty() || decoration.amount <= 0 {
            return;
        }
        // sub_10004BAB4 reaches GameLua+0xe0 (ResourceManager) only after the
        // RenderObjectData+0x141 decoration byte and its positive count have
        // passed. Ordinary sub_10006D5B4 submissions use the retained
        // +0x78/+0x90 pointers, so their caller must not acquire the rehost's
        // shared resource-manager lock.
        let Some((resources, data_root)) = decoration_resources else {
            return;
        };
        let mut decoration_angle = object.angle as f32;
        let radians_per_step = (decoration.angle_increment as f32) * f32::from_bits(0x4049_0FDB);
        for _ in 0..decoration.amount {
            let decoration_scale = decoration.scale as f32;
            let scale_x = object.scale_x as f32 * decoration_scale;
            let scale_y = object.scale_y as f32 * decoration_scale;
            let world_scale = self.world_scale as f32;
            let bound_region = resources.active_atlas_catalog_region(&decoration.sprite, data_root);
            let bound_composite = resources.active_bound_composite(&decoration.sprite);
            let (pivot_x, pivot_y) = native_scene_callback_pivot(
                bound_composite.as_deref(),
                bound_region.as_deref(),
                0.0,
                0.0,
            );
            let decoration_state = RenderState {
                translate_x: f64::from(-(self.top_left_x as f32) / scale_x),
                translate_y: f64::from(-(self.top_left_y as f32) / scale_y),
                scale_x: f64::from(world_scale * scale_x),
                scale_y: f64::from(world_scale * scale_y),
                angle: f64::from(decoration_angle),
                matrix: None,
                masked_texture_matrix: None,
                sprite_pivot: None,
                pivot_x: f64::from(pivot_x),
                pivot_y: f64::from(pivot_y),
                draw_size: None,
                alpha: object.alpha,
                clip_rect: self.state.clip_rect,
            };
            // 0x10004C278..0x10004C2C4 installs the divided camera
            // translation and raw object*decoration scale, then calls the
            // shared ResourceManager HPIVOT/VPIVOT draw with the divided
            // object position. It deliberately does not inherit the
            // ordinary body's horizontal flip or game-world body scale.
            self.state = decoration_state;
            if let Some(command) = native_resource_sprite_command(
                resources,
                data_root,
                ParsedSpriteDraw {
                    sprite: decoration.sprite.clone(),
                    x: f64::from((object.x as f32 * 20.0_f32) / scale_x),
                    y: f64::from((object.y as f32 * 20.0_f32) / scale_y),
                    horizontal_anchor: SpriteHorizontalAnchor::Pivot,
                    vertical_anchor: SpriteVerticalAnchor::Pivot,
                    draw_size: None,
                },
                decoration_state,
            ) {
                self.push_render_command(command);
            }
            // 0x10004C2CC..0x10004C2EC performs one rounded FMUL followed
            // by FMADD and fmodf. `angleIncrement` is authored in degrees;
            // the old host path incorrectly treated it as radians.
            decoration_angle = radians_per_step
                .mul_add(f32::from_bits(0x3BB6_0B61), decoration_angle)
                % (f32::from_bits(0x4049_0FDB) + f32::from_bits(0x4049_0FDB));
            if decoration_angle < 0.0 {
                decoration_angle += f32::from_bits(0x4049_0FDB) + f32::from_bits(0x4049_0FDB);
            }
        }
        // The post callback therefore sees the last iteration's state. The
        // loop computes one following angle but never stores it into GL.
    }
}
