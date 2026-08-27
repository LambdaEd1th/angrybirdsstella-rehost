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
        lua.create_function(move |_, args: mlua::MultiValue| {
            // sub_100084628 preserves exact STRING, TABLE and NUMBER
            // tags at slots 1..7 and ignores any trailing Lua values.
            let sprite = native_required_borrowed_string(&args, 0, "drawSpriteWithShader")?;
            let shader = native_required_table(&args, 1, "drawSpriteWithShader")?;
            let x = native_required_number(&args, 2, "drawSpriteWithShader")?;
            let y = native_required_number(&args, 3, "drawSpriteWithShader")?;
            let scale_x = native_required_number(&args, 4, "drawSpriteWithShader")?;
            let scale_y = native_required_number(&args, 5, "drawSpriteWithShader")?;
            let angle = native_required_number(&args, 6, "drawSpriteWithShader")?;
            let mut resources = resource_runtime
                .lock()
                .expect("resource runtime lock poisoned");
            let shader = sprite_shader_from_lua(shader, &mut resources.shader_cache)?;
            let atlas_binding = resources.active_atlas_draw_binding(sprite, &data_root);
            let parts = atlas_binding
                .is_none()
                .then(|| resources.active_bound_composite_snapshot(sprite))
                .flatten();
            drop(resources);
            let mut bridge = render.lock().expect("render bridge lock poisoned");
            let state = bridge.state;
            if let Some((sprite_name, bound_region)) = atlas_binding {
                bridge.push_render_command(native_direct_sprite_command(
                    sprite_name,
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
            let Some(parts) = parts else {
                return Ok(());
            };

            // The member falls back to the composite record only when
            // atlas lookup fails. It ignores part scale, flip and
            // visibility, offsets with the caller transform, subtracts
            // the stored part angle, and skips nested composites.
            let cosine = angle.cos();
            let sine = angle.sin();
            for bound in parts.iter() {
                let part = &bound.part;
                let part_x = f64::from(part.x) * scale_x;
                let part_y = f64::from(part.y) * scale_y;
                bridge.push_render_command(native_direct_sprite_command(
                    bound.sprite.clone(),
                    Arc::clone(&bound.region),
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
        })?,
    )
}
