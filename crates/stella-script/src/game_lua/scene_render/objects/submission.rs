//! Ordinary sprite, ray and decoration submission owned by `sub_10006D5B4`.

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
        object: &SceneDrawObject,
        decoration_resources: Option<(&ResourceRuntime, &Path)>,
        shader: Option<SpriteShader>,
    ) {
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
        if let Some(mut command) = self.scene_object_command(object) {
            // sub_10006D5B4 looks up the live Lua object's `shader` field at
            // 0x10006D6D8..0x10006D744 immediately before submitting either
            // its ordinary sprite or every composite part. The shader is not
            // a RenderObjectData member and therefore must be supplied from
            // the dispatcher on every draw.
            command.shader = shader.map(Arc::new);
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
        let bound_region = resources
            .active_atlas_catalog_region(&decoration.sprite, data_root)
            .map(Arc::new);
        let mut bound_composite = resources
            .active_bound_composite(&decoration.sprite)
            .map(Arc::new);
        if bound_region.is_none() && bound_composite.is_none() {
            bound_composite = Some(Arc::new(Vec::new()));
        }
        let sprite: SharedSpriteName = decoration.sprite.as_str().into();
        let base = self.scene_object_state(object);
        for index in 0..decoration.amount {
            self.push_render_command(RenderCommand {
                order: 0,
                sprite: sprite.clone(),
                texture: None,
                bound_region: bound_region.clone(),
                bound_composite: bound_composite.clone(),
                geometry: None,
                shader: None,
                dirt: None,
                x: 0.0,
                y: 0.0,
                state: RenderState {
                    scale_x: base.scale_x * decoration.scale,
                    scale_y: base.scale_y * decoration.scale,
                    angle: base.angle + decoration.angle_increment * index as f64,
                    ..base
                }
                .into(),
                world_space: true,
            });
        }
    }
}
