//! Direct LuaState ThemeAnimation table parser (`sub_100055650`).

use crate::*;

pub(crate) fn install(
    lua: &Lua,
    globals: &mlua::Table,
    render: Arc<Mutex<RenderBridge>>,
) -> LuaResult<()> {
    globals.set(
        "createThemeAnimation",
        lua.create_function(move |_, args: MultiValue| {
            // Unlike the generated ThemeSprite adapters, sub_100055650 checks
            // Lua's current stack top. An earlier table does not rescue a
            // non-table final argument.
            let Some(table) = args.iter().last().and_then(|value| match value {
                Value::Table(table) => Some(table.clone()),
                _ => None,
            }) else {
                return Ok(());
            };

            let name = theme_table_string(&table, "name")?.unwrap_or_default();
            let sprite = theme_table_string(&table, "spriteName")?.unwrap_or_default();
            let layer_index = native_theme_layer(theme_table_f32(&table, "layer")?.unwrap_or(0.0));
            let x = theme_table_f32(&table, "x")?.unwrap_or(0.0);
            let y = theme_table_f32(&table, "y")?.unwrap_or(0.0);
            let animation_frames = match table.get::<Value>("animation")? {
                Value::Table(frames) => {
                    let mut values = Vec::new();
                    for index in 1.. {
                        let value = frames.raw_get::<Value>(index)?;
                        // LuaState::isString (`sub_10052811C`) accepts both
                        // type tags NUMBER and STRING; lua_tolstring then owns
                        // the exact conversion. Any other tag terminates the
                        // native one-based scan.
                        let Some(value) = native_lua51_string(&value) else {
                            break;
                        };
                        values.push(value);
                    }
                    values
                }
                _ => Vec::new(),
            };
            let animation_frame_time = theme_table_f32(&table, "animDelay")?.unwrap_or(0.0);
            let animation_timer =
                theme_table_f32(&table, "startingDelay")?.unwrap_or(animation_frame_time);

            render
                .lock()
                .expect("render bridge lock poisoned")
                .theme_sprites
                .insert(
                    (layer_index, name),
                    NativeThemeSprite {
                        sprite,
                        x: f64::from(x),
                        y: f64::from(y),
                        scale_x: f64::from(theme_table_f32(&table, "scaleX")?.unwrap_or(1.0)),
                        scale_y: f64::from(theme_table_f32(&table, "scaleY")?.unwrap_or(1.0)),
                        angle: f64::from(theme_table_f32(&table, "angle")?.unwrap_or(0.0)),
                        angular_velocity: 0.0,
                        horizontal_flip: false,
                        velocity_x: f64::from(theme_table_f32(&table, "velX")?.unwrap_or(0.0)),
                        velocity_y: f64::from(theme_table_f32(&table, "velY")?.unwrap_or(0.0)),
                        scale_speed: f64::from(
                            theme_table_f32(&table, "scaleSpeed")?.unwrap_or(0.0),
                        ),
                        original_x: f64::from(x),
                        original_y: f64::from(y),
                        animation_start_timer: f64::from(
                            theme_table_f32(&table, "startAnimTimer")?.unwrap_or(0.0),
                        ),
                        is_animation: theme_table_bool(&table, "isAnimation")?.unwrap_or(false),
                        animation_frames,
                        animation_frame_time: f64::from(animation_frame_time),
                        animation_timer: f64::from(animation_timer),
                        animation_frame: 0,
                        animation_looping: theme_table_bool(&table, "bLoop")?.unwrap_or(false),
                    },
                );
            Ok(())
        })?,
    )?;
    Ok(())
}
