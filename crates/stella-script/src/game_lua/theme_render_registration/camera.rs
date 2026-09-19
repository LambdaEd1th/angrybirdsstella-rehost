//! ThemeSystem camera accessors and the lazy draw prelude (0x10009AA4C).

use crate::*;

pub(crate) fn required_camera_table(parent: &mlua::Table, key: &str) -> LuaResult<mlua::Table> {
    required_camera_value(parent.raw_get(key)?, key)
}

pub(crate) fn required_camera_value(value: Value, key: &str) -> LuaResult<mlua::Table> {
    match value {
        Value::Table(table) => Ok(table),
        value => Err(runtime_error(format!(
            "Tried to get a Lua table from index '{key}', but type was {}",
            value.type_name()
        ))),
    }
}

/// LuaObject::getNumber calls lua_tonumber, not the generated strict adapter.
pub(crate) fn camera_number(camera: &mlua::Table, key: &str) -> LuaResult<f32> {
    Ok(native_lua51_number(&camera.raw_get::<Value>(key)?).unwrap_or(0.0) as f32)
}

/// sub_10009961C: absent gameCamera leaves +0xA8 unchanged. All table
/// traversals for a present camera are required raw lookups.
pub(crate) fn capture_end_scale(
    environment: &mlua::Table,
    bridge: &mut RenderBridge,
) -> LuaResult<()> {
    let value = environment.raw_get::<Value>("gameCamera")?;
    if matches!(value, Value::Nil) {
        return Ok(());
    }
    let camera = required_camera_value(value, "gameCamera")?;
    let cameras = required_camera_table(&camera, "resolutionCorrectedCameras")?;
    let index = native_fcvtzs_f32(camera_number(&camera, "endCameraIndex")?);
    let end = required_camera_value(cameras.raw_get(index)?, &index.to_string())?;
    bridge.resolution_camera_scale = camera_number(&end, "sx")?;
    Ok(())
}

pub(super) fn prepare_draw(
    lua: &Lua,
    bridge: &mut RenderBridge,
    foreground: bool,
) -> LuaResult<()> {
    // 0x10009BDEC..BE0C precedes the lazy call, even for empty passes.
    bridge.theme_camera.y = if foreground {
        0.0
    } else {
        bridge.theme_camera.saved_y
    };
    let environment = game_environment(lua)?;
    let screen = required_camera_table(&environment, "screen")?;
    bridge.theme_camera.screen_x = camera_number(&screen, "x")?;
    bridge.theme_camera.screen_y = camera_number(&screen, "y")?;
    bridge.theme_camera.current_scale = bridge.world_scale as f32;
    bridge.theme_camera.world_limits = ThemeWorldLimits {
        left: Some(camera_number(&environment, "leftLimitWorld")?),
        right: Some(camera_number(&environment, "rightLimitWorld")?),
        top: Some(camera_number(&environment, "topLimitWorld")?),
        bottom: Some(camera_number(&environment, "bottomLimitWorld")?),
    };
    if bridge.theme_camera.valid {
        return Ok(());
    }

    // 0x10009AD08 is deliberately before all fallible reference lookups.
    bridge.theme_camera.valid = true;
    let device =
        native_lua51_string(&environment.raw_get::<Value>("deviceModel")?).unwrap_or_default();
    let objects = required_camera_table(&environment, "objects")?;
    let castles = required_camera_table(&objects, "castleCameraData")?;
    let castle = required_camera_table(&castles, &device)?;
    let mut x = camera_number(&castle, "px")?;
    let mut y = camera_number(&castle, "py")?;
    // sub_1005280DC checks the BOOLEAN tag before getBoolean. Truthy
    // strings/numbers must not activate these optional switches.
    if environment.raw_get::<Value>("g_useLowerCameraAsThemeReferencePoint")?
        == Value::Boolean(true)
    {
        let birds = required_camera_table(&objects, "birdCameraData")?;
        let bird = required_camera_table(&birds, &device)?;
        let bird_x = camera_number(&bird, "px")?;
        let bird_y = camera_number(&bird, "py")?;
        // FCMP/B.LE also selects the bird when the operands are unordered.
        if x.partial_cmp(&bird_x) != Some(std::cmp::Ordering::Greater) {
            x = bird_x;
        }
        if y.partial_cmp(&bird_y) != Some(std::cmp::Ordering::Greater) {
            y = bird_y;
        }
    }
    if environment.raw_get::<Value>("g_useZeroAsThemeReferencePointX")? == Value::Boolean(true) {
        x = 0.0;
    }
    if environment.raw_get::<Value>("g_useZeroAsThemeReferencePointY")? == Value::Boolean(true) {
        y = 0.0;
    }
    bridge.theme_camera.x = x;
    bridge.theme_camera.y = y;
    bridge.theme_camera.saved_y = y;
    capture_end_scale(&environment, bridge)?;
    bridge.initialize_theme_world_offsets(foreground);
    Ok(())
}
