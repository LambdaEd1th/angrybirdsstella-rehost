//! Global GameLua `drawCompoSprite` (`sub_10004DDA0`).

use crate::*;

pub(super) fn install(
    lua: &Lua,
    globals: &mlua::Table,
    render: Arc<Mutex<RenderBridge>>,
    resource_runtime: Arc<Mutex<ResourceRuntime>>,
    _data_root: Arc<PathBuf>,
) -> LuaResult<()> {
    globals.set(
        "drawCompoSprite",
        lua.create_function(move |_, args: mlua::MultiValue| {
            // sub_100085C1C reads one exact STRING followed by four exact
            // NUMBER slots; generated adapters do not reject extras.
            let sprite = native_required_string(&args, 0, "drawCompoSprite")?;
            let x = native_required_number(&args, 1, "drawCompoSprite")?;
            let y = native_required_number(&args, 2, "drawCompoSprite")?;
            let local_scale_x = native_required_number(&args, 3, "drawCompoSprite")?;
            let local_scale_y = native_required_number(&args, 4, "drawCompoSprite")?;
            let resources = resource_runtime
                .lock()
                .expect("resource runtime lock poisoned");
            let Some((parts, regions)) = resources.active_composite_bound_parts(&sprite) else {
                return Ok(());
            };
            let parts = parts.to_vec();
            let regions = regions.to_vec();
            drop(resources);
            let mut bridge = render.lock().expect("render bridge lock poisoned");
            let current = bridge.state;

            // This old GameLua helper is distinct from
            // ResourceManager.drawCompoSprite. It draws each directly
            // referenced atlas sprite with only record x/y and the two
            // call scales; record scale, angle, flip and enabled are ignored.
            let cosine = current.angle.cos();
            let sine = current.angle.sin();
            // GL_Context applies stored scale after rotation. Preserve
            // Scale * Rotation * LocalScale explicitly for non-uniform scale.
            let m00 = current.scale_x * cosine * local_scale_x;
            let m01 = -current.scale_x * sine * local_scale_y;
            let m10 = current.scale_y * sine * local_scale_x;
            let m11 = current.scale_y * cosine * local_scale_y;

            for (part, bound_region) in parts.into_iter().zip(regions) {
                // The record stores an AtlasSprite pointer. Nested
                // composites have no such pointer and are skipped.
                let geometry = SpriteGeometry {
                    min_x: -f64::from(bound_region.sprite.pivot_x),
                    min_y: -f64::from(bound_region.sprite.pivot_y),
                    max_x: f64::from(bound_region.sprite.width)
                        - f64::from(bound_region.sprite.pivot_x),
                    max_y: f64::from(bound_region.sprite.height)
                        - f64::from(bound_region.sprite.pivot_y),
                };
                let part_x = f64::from(part.x);
                let part_y = f64::from(part.y);
                let pivot_x = -geometry.min_x;
                let pivot_y = -geometry.min_y;
                let native_pivot_x = (pivot_x - part_x) * local_scale_x;
                let native_pivot_y = (pivot_y - part_y) * local_scale_y;
                bridge.state.pivot_x = native_pivot_x;
                bridge.state.pivot_y = native_pivot_y;
                bridge.push_render_command(RenderCommand {
                    order: 0,
                    sprite: part.sprite.into(),
                    texture: None,
                    texture_scale: 1.0,
                    masked_texture_binding: None,
                    bound_region: Some(bound_region.into()),
                    bound_composite: None,
                    shader: None,
                    clip_holes: Vec::new(),
                    dirt: None,
                    x: current.scale_x
                        * (current.translate_x + x + cosine * local_scale_x * part_x
                            - sine * local_scale_y * part_y),
                    y: current.scale_y
                        * (current.translate_y
                            + y
                            + sine * local_scale_x * part_x
                            + cosine * local_scale_y * part_y),
                    state: RenderState {
                        scale_x: current.scale_x * local_scale_x,
                        scale_y: current.scale_y * local_scale_y,
                        angle: current.angle,
                        matrix: Some([m00, m01, m10, m11]),
                        pivot_x: native_pivot_x,
                        pivot_y: native_pivot_y,
                        alpha: current.alpha,
                        clip_rect: current.clip_rect,
                        ..RenderState::default()
                    },
                    world_space: true,
                });
            }
            Ok(())
        })?,
    )
}
