//! `drawSpriteWithoutShader` (`sub_10004E300`).

use crate::*;

pub(super) fn install(
    lua: &Lua,
    globals: &mlua::Table,
    render: Arc<Mutex<RenderBridge>>,
    resource_runtime: Arc<Mutex<ResourceRuntime>>,
    data_root: Arc<PathBuf>,
) -> LuaResult<()> {
    globals.set(
        "drawSpriteWithoutShader",
        lua.create_function(
            move |_, (sprite, x, y, scale_x, scale_y, angle): (String, f64, f64, f64, f64, f64)| {
                // The member asks only the atlas-sprite cache; unlike the
                // shader path it has no composite fallback.
                let Some(bound_region) = resource_runtime
                    .lock()
                    .expect("resource runtime lock poisoned")
                    .active_atlas_catalog_region(&sprite, &data_root)
                else {
                    return Ok(());
                };
                let mut bridge = render.lock().expect("render bridge lock poisoned");
                let state = bridge.state;
                bridge.push_render_command(native_direct_sprite_command(
                    sprite,
                    bound_region,
                    None,
                    NativeSpritePlacement {
                        x,
                        y,
                        scale_x,
                        scale_y,
                        angle,
                    },
                    state,
                ));
                Ok(())
            },
        )?,
    )
}
