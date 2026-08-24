//! `drawSpriteWithShader` (`sub_10004E070`).

use crate::*;

pub(super) fn install(
    lua: &Lua,
    globals: &mlua::Table,
    render: Arc<Mutex<RenderBridge>>,
    resource_runtime: Arc<Mutex<ResourceRuntime>>,
    data_root: Arc<PathBuf>,
) -> LuaResult<()> {
    globals.set(
        "drawSpriteWithShader",
        lua.create_function(
            move |_,
                  (sprite, shader, x, y, scale_x, scale_y, angle): (
                String,
                mlua::Table,
                f64,
                f64,
                f64,
                f64,
                f64,
            )| {
                let mut resources = resource_runtime
                    .lock()
                    .expect("resource runtime lock poisoned");
                let shader = sprite_shader_from_lua(shader, &mut resources.shader_cache)?;
                let atlas_region = resources.active_atlas_catalog_region(&sprite, &data_root);
                let parts = atlas_region
                    .is_none()
                    .then(|| {
                        resources
                            .active_composite_bound_parts(&sprite)
                            .map(|(parts, regions)| (parts.to_vec(), regions.to_vec()))
                    })
                    .flatten();
                drop(resources);
                let mut bridge = render.lock().expect("render bridge lock poisoned");
                let state = bridge.state;
                if let Some(bound_region) = atlas_region {
                    bridge.push_render_command(native_direct_sprite_command(
                        sprite,
                        bound_region,
                        Some(shader),
                        NativeSpritePlacement {
                            x,
                            y,
                            scale_x,
                            scale_y,
                            angle,
                        },
                        state,
                    ));
                    return Ok(());
                }
                let Some((parts, atlas_parts)) = parts else {
                    return Ok(());
                };

                // The member falls back to the composite record only when
                // atlas lookup fails. It ignores part scale, flip and
                // visibility, offsets with the caller transform, subtracts
                // the stored part angle, and skips nested composites.
                let cosine = angle.cos();
                let sine = angle.sin();
                for (part, bound_region) in parts.into_iter().zip(atlas_parts) {
                    let part_x = f64::from(part.x) * scale_x;
                    let part_y = f64::from(part.y) * scale_y;
                    bridge.push_render_command(native_direct_sprite_command(
                        part.sprite,
                        bound_region,
                        Some(shader.clone()),
                        NativeSpritePlacement {
                            x: x + cosine * part_x - sine * part_y,
                            y: y + sine * part_x + cosine * part_y,
                            scale_x,
                            scale_y,
                            angle: angle - f64::from(part.angle),
                        },
                        state,
                    ));
                }
                Ok(())
            },
        )?,
    )
}
