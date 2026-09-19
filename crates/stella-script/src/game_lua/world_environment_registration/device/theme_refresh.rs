//! `native_refreshThemeSystem` / `sub_1000985DC`.
//! Reference scales and layer caches are independent of draw's lazy XY latch.

use crate::game_lua::theme_render_registration::camera::{
    camera_number, capture_end_scale, required_camera_table, required_camera_value,
};
use crate::*;

mod foreground_offsets;

use foreground_offsets::{ResolutionCamera, resolve_symbolic_foreground_offsets};

pub(super) fn install(
    lua: &Lua,
    globals: &mlua::Table,
    render: Arc<Mutex<RenderBridge>>,
) -> LuaResult<()> {
    globals.set(
        "native_refreshThemeSystem",
        lua.create_function(move |lua, _: MultiValue| {
            let environment = game_environment(lua)?;
            let objects = required_camera_table(&environment, "objects")?;
            let castles = required_camera_table(&objects, "castleCameraData")?;
            let reference = match castles.raw_get::<Value>("ipad")? {
                Value::Table(camera) => camera,
                _ => required_camera_table(&castles, "referenceCamera")?,
            };
            let originals = match environment.raw_get::<Value>("originalCameras")? {
                Value::Table(cameras) => cameras,
                _ => required_camera_table(
                    &required_camera_table(&environment, "gameCamera")?,
                    "originalCameras",
                )?,
            };
            let original = required_camera_value(originals.raw_get(2_i64)?, "2")?;
            let original_scale = camera_number(&original, "sx")?;
            // Native reads sy as well, although only the original sx is retained.
            let _ = camera_number(&original, "sy")?;
            let reference_scale = camera_number(&reference, "sx")?;
            let reference_scale_y = camera_number(&reference, "sy")?;

            let mut bridge = render.lock().expect("render bridge lock poisoned");
            // 0x100098958..974 precedes spawner rebuilding. No XY/reference
            // initialization and no finite-value guard occur in this member.
            bridge.theme_camera.scale = reference_scale;
            bridge.theme_camera.scale_y = reference_scale_y;
            bridge.theme_camera.original_scale_ratio = original_scale / reference_scale;
            let background_layers = bridge.theme_background_layers.clone();
            let foreground_layers = bridge.theme_foreground_layers.clone();
            bridge.theme_background_particles.clear();
            bridge.theme_foreground_particles.clear();
            for layer in &background_layers {
                bridge
                    .theme_background_particles
                    .install_layer_spawner(lua, layer, 2)?;
            }
            for layer in &foreground_layers {
                bridge
                    .theme_foreground_particles
                    .install_layer_spawner(lua, layer, 1)?;
            }
            capture_end_scale(&environment, &mut bridge)?;
            drop(bridge);
            resolve_symbolic_foreground_offsets(lua, &render)?;
            Ok(())
        })?,
    )
}

fn resolution_camera_data(environment: &mlua::Table) -> LuaResult<Vec<ResolutionCamera>> {
    // sub_100099828 uses raw integer indexing and stops only at nil; a
    // non-table entry is a required-table error rather than a truncated list.
    let camera = required_camera_table(environment, "gameCamera")?;
    let cameras = required_camera_table(&camera, "resolutionCorrectedCameras")?;
    let mut result = Vec::new();
    for index in 1_i64.. {
        let value = cameras.raw_get::<Value>(index)?;
        if matches!(value, Value::Nil) {
            break;
        }
        let camera = required_camera_value(value, &index.to_string())?;
        result.push(ResolutionCamera {
            scale: camera_number(&camera, "sx")?,
            x: camera_number(&camera, "px")?,
            y: camera_number(&camera, "py")?,
            left: camera_number(&camera, "left")?,
            top: camera_number(&camera, "top")?,
        });
    }
    Ok(result)
}
