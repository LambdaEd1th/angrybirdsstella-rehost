//! Ordinary sprite, ray and decoration submission owned by `sub_10006D5B4`.

use super::SceneDrawObject;
use crate::*;

impl RenderBridge {
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
                sprite: object.sprite.clone(),
                texture: object.texture.clone(),
                texture_scale: object.texture_scale,
                masked_texture_binding: object.texture_binding.clone(),
                bound_region: object.sprite_region.clone(),
                bound_composite: object.composite_sprite.clone(),
                shader: None,
                clip_holes: object.dirt.as_ref().map_or_else(
                    || {
                        object
                            .dirt_holes
                            .iter()
                            .map(|hole| RenderHole {
                                x: hole.local_x * 20.0,
                                y: hole.local_y * 20.0,
                                radius: hole.radius * 20.0,
                            })
                            .collect()
                    },
                    |_| Vec::new(),
                ),
                dirt: object.dirt.as_ref().map(DirtComponent::render_command),
                x: 0.0,
                y: 0.0,
                state: self.scene_object_state(object),
                world_space: true,
            })
    }

    #[cfg(test)]
    pub(crate) fn draw_scene_range(&mut self) {
        let objects = self
            .scene_range_names()
            .into_iter()
            .filter_map(|name| self.scene_draw_object(&name))
            .filter(|object| object.visible)
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
        resources: &ResourceRuntime,
        data_root: &Path,
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
            command.shader = shader;
            self.push_render_command(command);
        }
        if object.flash_animation {
            return;
        }
        let Some(decoration) = object.decoration.as_ref() else {
            return;
        };
        if decoration.sprite.is_empty() || decoration.amount <= 0 {
            return;
        }
        let bound_region = resources.active_atlas_catalog_region(&decoration.sprite, data_root);
        let mut bound_composite = resources.active_bound_composite(&decoration.sprite);
        if bound_region.is_none() && bound_composite.is_none() {
            bound_composite = Some(Vec::new());
        }
        let base = self.scene_object_state(object);
        for index in 0..decoration.amount {
            self.push_render_command(RenderCommand {
                order: 0,
                sprite: decoration.sprite.clone(),
                texture: None,
                texture_scale: 1.0,
                masked_texture_binding: None,
                bound_region: bound_region.clone(),
                bound_composite: bound_composite.clone(),
                shader: None,
                clip_holes: Vec::new(),
                dirt: None,
                x: 0.0,
                y: 0.0,
                state: RenderState {
                    scale_x: base.scale_x * decoration.scale,
                    scale_y: base.scale_y * decoration.scale,
                    angle: base.angle + decoration.angle_increment * index as f64,
                    ..base
                },
                world_space: true,
            });
        }
    }
}
