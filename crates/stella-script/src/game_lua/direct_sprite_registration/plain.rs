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
        lua.create_function(move |_, args: mlua::MultiValue| {
            // Generated adapter sub_100084398 reads one exact STRING
            // followed by five exact NUMBER slots.  In particular, the
            // stock mlua String/f64 tuple would accept coercions that
            // Purple rejects through sub_1005285CC/sub_10052859C.
            let sprite = native_required_string(&args, 0, "drawSpriteWithoutShader")?;
            let x = native_required_number(&args, 1, "drawSpriteWithoutShader")?;
            let y = native_required_number(&args, 2, "drawSpriteWithoutShader")?;
            let scale_x = native_required_number(&args, 3, "drawSpriteWithoutShader")?;
            let scale_y = native_required_number(&args, 4, "drawSpriteWithoutShader")?;
            let angle = native_required_number(&args, 5, "drawSpriteWithoutShader")?;
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
        })?,
    )
}
