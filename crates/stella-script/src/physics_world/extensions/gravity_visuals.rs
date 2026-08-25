//! Authored sensor-gravity visuals from GameLua `sub_100032DC4`.

use crate::*;

const FUNCTION: &str = "renderGravityVisualsNative";
const BOX_FADED_SPRITE: &str = "THEME_1_GRAVITY_SLICE_BOX_FADED";
// Purple stores this literal in the recovered instruction stream; replacing it
// with Rust's more precise PI would change the submitted float32 rotations.
#[allow(clippy::approx_constant)]
const NATIVE_PI: f32 = 3.1416_f32;

pub(super) fn install(
    lua: &Lua,
    globals: &mlua::Table,
    render: &Arc<Mutex<RenderBridge>>,
    resources: Arc<Mutex<ResourceRuntime>>,
    data_root: Arc<PathBuf>,
) -> LuaResult<()> {
    let draw_bridge = Arc::clone(render);
    globals.set(
        FUNCTION,
        lua.create_function(move |lua, args: MultiValue| {
            // The hand-written member wraps argument one as LuaObject but its
            // generated float arguments retain exact NUMBER-tag checks.
            let sensor = native_required_table(&args, 0, FUNCTION)?;
            let screen_x = native_required_number(&args, 1, FUNCTION)? as f32;
            let screen_y = native_required_number(&args, 2, FUNCTION)? as f32;
            let world_scale = native_required_number(&args, 3, FUNCTION)? as f32;

            // These four tests intentionally inspect only the Lua types. In
            // particular, `active=false` is still eligible and Lua 5.1's
            // isstring/isnumber predicates accept convertible numbers/strings.
            if native_lua51_string(&sensor.get::<Value>("sensorType")?).is_none()
                || native_lua51_number(&sensor.get::<Value>("addVisualTimer")?).is_none()
                || !matches!(sensor.get::<Value>("active")?, Value::Boolean(_))
            {
                return Ok(());
            }
            let Some(definition_name) = native_lua51_string(&sensor.get::<Value>("definition")?)
            else {
                return Ok(());
            };

            let shape = block_shape(lua, &definition_name)?;
            let box_shape = if shape == "circle" {
                false
            } else {
                // Purple performs the blockTable lookup a second time for
                // every non-circle definition before comparing with box.
                // Preserve that observable metatable access order.
                block_shape(lua, &definition_name)? == "box"
            };
            if shape != "circle" && !box_shape {
                return Ok(());
            }
            let resources = resources.lock().expect("resource runtime lock poisoned");
            let mut bridge = draw_bridge.lock().expect("render bridge lock poisoned");
            if shape == "circle" {
                render_circle(
                    &sensor,
                    screen_x,
                    screen_y,
                    world_scale,
                    &resources,
                    &data_root,
                    &mut bridge,
                )?;
            } else {
                render_box(
                    &sensor,
                    screen_x,
                    screen_y,
                    world_scale,
                    &resources,
                    &data_root,
                    &mut bridge,
                )?;
            }
            Ok(())
        })?,
    )
}

fn block_shape(lua: &Lua, definition_name: &str) -> LuaResult<String> {
    let block_table = native_lua_object(lua, NativeLuaObject::BlockTable)?
        .ok_or_else(|| runtime_error(format!("{FUNCTION} blockTable is not available")))?;
    let blocks = required_table_field(&block_table, "blocks")?;
    let definition = match blocks.get::<Value>(definition_name)? {
        Value::Table(definition) => definition,
        value => {
            return Err(runtime_error(format!(
                "{FUNCTION} block definition {definition_name:?} must be table, found {}",
                describe_value(&value)
            )));
        }
    };
    Ok(native_lua51_string(&definition.get::<Value>("type")?).unwrap_or_default())
}

fn render_circle(
    sensor: &mlua::Table,
    screen_x: f32,
    screen_y: f32,
    world_scale: f32,
    resources: &ResourceRuntime,
    data_root: &Path,
    bridge: &mut RenderBridge,
) -> LuaResult<()> {
    let radius = lua_number_field(sensor, "radius")?;
    let visuals = required_table_field(sensor, "gravityVisuals")?;
    // 0x100033674..0x1000336A0: the two first products are float32,
    // while 0.019 is a binary64 constant followed by FCVT back to float32.
    let radius_scale = world_scale * (radius * 3.0_f32);
    let factor = (f64::from(radius_scale) * 0.019_f64) as f32;

    let mut index = 1_u32;
    loop {
        let entry = match visuals.raw_get::<Value>(index)? {
            Value::Nil => break,
            Value::Table(entry) => entry,
            value => {
                return Err(runtime_error(format!(
                    "{FUNCTION} gravityVisuals[{index}] must be table, found {}",
                    describe_value(&value)
                )));
            }
        };
        let entry_scale = lua_number_field(&entry, "scale")?;
        let x = lua_number_field(&entry, "x")?;
        let y = lua_number_field(&entry, "y")?;
        let sprite = lua_string_field(&entry, "sprite")?;
        let metrics = sprite_metrics(resources, &sprite);
        let scale = factor * entry_scale;
        bridge.state.translate_x = f64::from(screen_x / scale);
        bridge.state.translate_y = f64::from(screen_y / scale);
        bridge.state.scale_x = f64::from(scale);
        bridge.state.scale_y = f64::from(scale);
        bridge.state.pivot_x = f64::from(metrics.pivot_x);
        bridge.state.pivot_y = f64::from(metrics.pivot_y);

        for rotation in 0..4 {
            let angle = ((rotation as f32) * NATIVE_PI) * 0.5_f32;
            set_native_angle(&mut bridge.state, angle);
            submit_pivot_sprite(resources, data_root, bridge, &sprite, x, y);
        }
        let diagonal = NATIVE_PI * 0.25_f32;
        for rotation in 0..4 {
            let product = (rotation as f32) * NATIVE_PI;
            let angle = f64::from(product).mul_add(0.5_f64, f64::from(diagonal)) as f32;
            set_native_angle(&mut bridge.state, angle);
            submit_pivot_sprite(resources, data_root, bridge, &sprite, x, y);
        }
        index = index.wrapping_add(1);
    }
    Ok(())
}

fn render_box(
    sensor: &mlua::Table,
    screen_x: f32,
    screen_y: f32,
    world_scale: f32,
    resources: &ResourceRuntime,
    data_root: &Path,
    bridge: &mut RenderBridge,
) -> LuaResult<()> {
    let width = lua_number_field(sensor, "width")?;
    let height = lua_number_field(sensor, "height")?;
    let faded = sprite_metrics(resources, BOX_FADED_SPRITE);
    // sub_10045CD60 is the second getSpriteBounds component: height.
    let local_scale = (width * 20.0_f32) / faded.height as f32;
    let angle = lua_number_field(sensor, "angle")?;
    let (sine, cosine) = angle.sin_cos();
    let scale = world_scale * local_scale;
    bridge.state.translate_x = f64::from(screen_x / scale);
    bridge.state.translate_y = f64::from(screen_y / scale);
    bridge.state.scale_x = f64::from(scale);
    bridge.state.scale_y = f64::from(scale);
    let render_angle = f64::from(NATIVE_PI).mul_add(0.5_f64, f64::from(angle)) as f32;
    set_native_angle(&mut bridge.state, render_angle);
    bridge.state.pivot_x = f64::from(faded.pivot_x);
    bridge.state.pivot_y = f64::from(faded.pivot_y);

    let visuals = required_table_field(sensor, "gravityVisuals")?;
    let height_ten = height * 10.0_f32;
    let y_base = height_ten * cosine;
    let y_delta = (-y_base) - y_base;
    let x_base = height_ten * sine;
    let x_span = (height * 20.0_f32) * sine;
    let mut index = 1_u32;
    loop {
        let entry = match visuals.raw_get::<Value>(index)? {
            Value::Nil => break,
            Value::Table(entry) => entry,
            value => {
                return Err(runtime_error(format!(
                    "{FUNCTION} gravityVisuals[{index}] must be table, found {}",
                    describe_value(&value)
                )));
            }
        };
        let position = lua_number_field(&entry, "pos")?;
        let sprite = lua_string_field(&entry, "sprite")?;
        // 0x1000335A8..0x1000335B4 uses FMADD/FNMSUB before the two
        // divisions. S0 is the sine-derived X coordinate and S1 the
        // cosine-derived Y coordinate in the ResourceManager call ABI.
        let x = x_span.mul_add(position, -x_base) / local_scale;
        let y = y_delta.mul_add(position, y_base) / local_scale;
        submit_pivot_sprite(resources, data_root, bridge, &sprite, x, y);
        index = index.wrapping_add(1);
    }
    Ok(())
}

fn submit_pivot_sprite(
    resources: &ResourceRuntime,
    data_root: &Path,
    bridge: &mut RenderBridge,
    sprite: &str,
    x: f32,
    y: f32,
) {
    let draw = ParsedSpriteDraw {
        sprite: sprite.to_owned(),
        x: f64::from(x),
        y: f64::from(y),
        horizontal_anchor: SpriteHorizontalAnchor::Pivot,
        vertical_anchor: SpriteVerticalAnchor::Pivot,
        draw_size: None,
    };
    if let Some(command) = native_resource_sprite_command(resources, data_root, draw, bridge.state)
    {
        bridge.push_render_command(command);
    }
}

fn set_native_angle(state: &mut RenderState, angle: f32) {
    state.angle = f64::from(angle);
    // The native member rewrites GL_Context's cached sin/cos matrix. `None`
    // is this host's semantic equivalent: it rebuilds Scale*Rotation while
    // retaining the separate live pivot correction.
    state.matrix = None;
}

fn sprite_metrics(resources: &ResourceRuntime, sprite: &str) -> NativeSpriteMetrics {
    resources
        .active_native_sprite_metrics(sprite)
        .unwrap_or(NativeSpriteMetrics {
            width: 0,
            height: 0,
            pivot_x: 0,
            pivot_y: 0,
        })
}

fn required_table_field(table: &mlua::Table, field: &str) -> LuaResult<mlua::Table> {
    match table.get::<Value>(field)? {
        Value::Table(value) => Ok(value),
        value => Err(runtime_error(format!(
            "{FUNCTION} table field {field} must be table, found {}",
            describe_value(&value)
        ))),
    }
}

fn lua_number_field(table: &mlua::Table, field: &str) -> LuaResult<f32> {
    Ok(native_lua51_number(&table.get::<Value>(field)?).unwrap_or(0.0) as f32)
}

fn lua_string_field(table: &mlua::Table, field: &str) -> LuaResult<String> {
    Ok(native_lua51_string(&table.get::<Value>(field)?).unwrap_or_default())
}
