//! `native_refreshThemeSystem` / `sub_1000985DC`.

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
            // ThemeManager resolves `objects` by name from its owning GameLua
            // table (`sub_1000985DC` -> `sub_100072DC0`), rather than reading
            // GameLua's retained +0x408 LuaObject.
            let Value::Table(objects) = environment.get::<Value>("objects")? else {
                return Ok(());
            };
            let Value::Table(castle_cameras) = objects.get::<Value>("castleCameraData")? else {
                return Ok(());
            };

            // sub_1000985DC checks specifically for the iPad camera before
            // falling back to the authored referenceCamera record.
            let reference_camera = match castle_cameras.get::<Value>("ipad")? {
                Value::Table(camera) => Some(camera),
                _ => match castle_cameras.get::<Value>("referenceCamera")? {
                    Value::Table(camera) => Some(camera),
                    _ => None,
                },
            };
            let Some(reference_scale) = reference_camera
                .as_ref()
                .and_then(|camera| camera.get::<Value>("sx").ok())
                .as_ref()
                .and_then(native_lua51_number)
                .map(|value| value as f32)
            else {
                return Ok(());
            };

            let device_model = environment
                .get::<Value>("deviceModel")
                .ok()
                .as_ref()
                .and_then(native_lua51_string)
                .unwrap_or_else(|| "ios".to_owned());
            let Value::Table(castle_camera) = castle_cameras.get::<Value>(device_model.as_str())?
            else {
                return Ok(());
            };
            let Some(mut reference_x) = camera_number(&castle_camera, "px")? else {
                return Ok(());
            };
            let Some(mut reference_y) = camera_number(&castle_camera, "py")? else {
                return Ok(());
            };

            let lua_truthy = |value: &Value| !matches!(value, Value::Nil | Value::Boolean(false));
            if environment
                .get::<Value>("g_useLowerCameraAsThemeReferencePoint")
                .ok()
                .as_ref()
                .is_some_and(lua_truthy)
                && let Value::Table(bird_cameras) = objects.get::<Value>("birdCameraData")?
                && let Value::Table(bird_camera) =
                    bird_cameras.get::<Value>(device_model.as_str())?
            {
                if let Some(bird_x) = camera_number(&bird_camera, "px")? {
                    // 0x10009B040 branches to the bird value when
                    // castle <= bird: despite the setting's historical name,
                    // Purple selects the component-wise maximum.
                    reference_x = reference_x.max(bird_x);
                }
                if let Some(bird_y) = camera_number(&bird_camera, "py")? {
                    reference_y = reference_y.max(bird_y);
                }
            }
            if environment
                .get::<Value>("g_useZeroAsThemeReferencePointX")
                .ok()
                .as_ref()
                .is_some_and(lua_truthy)
            {
                reference_x = 0.0;
            }
            if environment
                .get::<Value>("g_useZeroAsThemeReferencePointY")
                .ok()
                .as_ref()
                .is_some_and(lua_truthy)
            {
                reference_y = 0.0;
            }

            // sub_10009961C refreshes +0xA8 from the corrected end camera as
            // part of the same native call. sub_100099828 then walks the same
            // array to resolve symbolic foreground offsets.
            let (resolution_cameras, resolution_scale) = resolution_camera_data(&environment)?;
            let original_scale_ratio = original_camera_scale(&environment)?
                .map(|original_scale| original_scale / reference_scale)
                .unwrap_or(1.0);

            // 0x1000989D4..0x100098A0C invokes sub_100096E4C on both existing
            // ThemeParticleSystems, then sub_100099168 walks the currently
            // expanded background and foreground layer arrays. The clear is
            // deliberately in-place: it retains each base Particles
            // definition tree at +0x60 while erasing the live/spawner trees.
            // `setTheme` itself does not touch the particle systems, so
            // selection and refresh remain observably separate as well.
            let (background_layers, foreground_layers) = {
                let bridge = render.lock().expect("render bridge lock poisoned");
                (
                    bridge.theme_background_layers.clone(),
                    bridge.theme_foreground_layers.clone(),
                )
            };

            let mut bridge = render.lock().expect("render bridge lock poisoned");
            bridge.theme_background_particles.clear();
            bridge.theme_foreground_particles.clear();
            for layer in &background_layers {
                // ThemeManager mode 1 is stored in the copied query as 2.
                bridge
                    .theme_background_particles
                    .install_layer_spawner(lua, layer, 2)?;
            }
            for layer in &foreground_layers {
                // ThemeManager mode 0 is stored in the copied query as 1.
                bridge
                    .theme_foreground_particles
                    .install_layer_spawner(lua, layer, 1)?;
            }

            bridge.theme_camera = ThemeCameraReference {
                valid: reference_scale.is_finite()
                    && reference_scale != 0.0
                    && reference_x.is_finite()
                    && reference_y.is_finite(),
                x: reference_x,
                y: reference_y,
                scale: reference_scale,
                original_scale_ratio,
                ..bridge.theme_camera
            };
            if let Some(scale) = resolution_scale.filter(|scale| scale.is_finite()) {
                bridge.resolution_camera_scale = scale;
            }
            resolve_symbolic_foreground_offsets(&mut bridge, &resolution_cameras);

            if std::env::var_os("STELLA_TRACE_THEME_CAMERA").is_some() {
                eprintln!(
                    "theme camera refresh: reference=({:.6},{:.6},{:.6}) end_scale={:.6}",
                    bridge.theme_camera.x,
                    bridge.theme_camera.y,
                    bridge.theme_camera.scale,
                    bridge.resolution_camera_scale
                );
            }
            Ok(())
        })?,
    )
}

fn original_camera_scale(environment: &mlua::Table) -> LuaResult<Option<f32>> {
    // 0x100098730..0x100098854: the global table wins when present;
    // otherwise Purple reads gameCamera.originalCameras. Both paths select
    // the one-based second camera and consume its `sx` member.
    let original_cameras = match environment.get::<Value>("originalCameras")? {
        Value::Table(cameras) => Some(cameras),
        _ => match environment.get::<Value>("gameCamera")? {
            Value::Table(game_camera) => match game_camera.get::<Value>("originalCameras")? {
                Value::Table(cameras) => Some(cameras),
                _ => None,
            },
            _ => None,
        },
    };
    let Some(original_cameras) = original_cameras else {
        return Ok(None);
    };
    let Value::Table(camera) = original_cameras.raw_get::<Value>(2_i64)? else {
        return Ok(None);
    };
    camera_number(&camera, "sx")
}

fn camera_number(camera: &mlua::Table, field: &str) -> LuaResult<Option<f32>> {
    let value = camera.get::<Value>(field)?;
    Ok(native_lua51_number(&value).map(|value| value as f32))
}

fn resolution_camera_data(
    environment: &mlua::Table,
) -> LuaResult<(Vec<ResolutionCamera>, Option<f32>)> {
    let Value::Table(game_camera) = environment.get::<Value>("gameCamera")? else {
        return Ok((Vec::new(), None));
    };
    let Value::Table(cameras) = game_camera.get::<Value>("resolutionCorrectedCameras")? else {
        return Ok((Vec::new(), None));
    };

    // sub_100099828 uses one-based raw indexing and stops at the first nil.
    let mut result = Vec::new();
    for index in 1_i64.. {
        match cameras.raw_get::<Value>(index)? {
            Value::Nil => break,
            Value::Table(camera) => result.push(ResolutionCamera {
                scale: camera_number_or_zero(&camera, "sx")?,
                x: camera_number_or_zero(&camera, "px")?,
                y: camera_number_or_zero(&camera, "py")?,
                left: camera_number_or_zero(&camera, "left")?,
                top: camera_number_or_zero(&camera, "top")?,
            }),
            _ => break,
        }
    }

    let end_scale = game_camera
        .get::<Value>("endCameraIndex")
        .ok()
        .as_ref()
        .and_then(native_lua51_number)
        .map(|index| native_fcvtzs_f32(index as f32))
        .and_then(|index| cameras.raw_get::<Value>(index).ok())
        .and_then(|camera| match camera {
            Value::Table(camera) => camera.get::<Value>("sx").ok(),
            _ => None,
        })
        .as_ref()
        .and_then(native_lua51_number)
        .map(|value| value as f32);
    Ok((result, end_scale))
}

fn camera_number_or_zero(camera: &mlua::Table, field: &str) -> LuaResult<f32> {
    Ok(camera_number(camera, field)?.unwrap_or(0.0))
}
