//! Five-argument native Align layout utility.

use crate::*;

pub(super) fn install(lua: &Lua, globals: &mlua::Table) -> LuaResult<()> {
    let align = lua.create_table()?;
    align.set(
        "getPositionAndScale",
        lua.create_function(
            |_,
             (layout, reference_width, reference_height, target_width, target_height): (
                mlua::Table,
                f64,
                f64,
                f64,
                f64,
            )| {
                let string = |field: &str| -> LuaResult<String> {
                    Ok(match layout.get::<Value>(field)? {
                        Value::String(value) => value.to_string_lossy(),
                        Value::Integer(value) => value.to_string(),
                        Value::Number(value) => (value as f32).to_string(),
                        _ => String::new(),
                    })
                };
                let number = |field: &str| -> LuaResult<f32> {
                    Ok(match layout.get::<Value>(field)? {
                        Value::Integer(value) => value as f32,
                        Value::Number(value) => value as f32,
                        Value::String(value) => value
                            .to_str()
                            .ok()
                            .and_then(|value| value.parse::<f32>().ok())
                            .unwrap_or(0.0),
                        _ => 0.0,
                    })
                };
                let scale_permissions = |mode: &str| match mode {
                    "TRUE" => (true, true),
                    "UP" => (true, false),
                    "DOWN" => (false, true),
                    _ => (false, false),
                };

                let (allow_x_up, allow_x_down) = scale_permissions(&string("scaleH")?);
                let (allow_y_up, allow_y_down) = scale_permissions(&string("scaleV")?);
                let mut ratio_x = target_width as f32 / reference_width as f32;
                let mut ratio_y = target_height as f32 / reference_height as f32;
                if (!allow_x_up && ratio_x > 1.0) || (!allow_x_down && ratio_x < 1.0) {
                    ratio_x = 1.0;
                }
                if (!allow_y_up && ratio_y > 1.0) || (!allow_y_down && ratio_y < 1.0) {
                    ratio_y = 1.0;
                }
                // sub_1000E0CB0 installs FIXED/NORMAL internally: FIXED
                // chooses the smaller axis ratio and NORMAL leaves it linear.
                if ratio_x >= ratio_y {
                    ratio_x = ratio_y;
                } else {
                    ratio_y = ratio_x;
                }

                let output_scale_x = number("scalex")? * ratio_x;
                let output_scale_y = number("scaley")? * ratio_y;
                let aligned =
                    |mode: &str, position: f32, target: f32, reference: f32, ratio: f32| match mode
                    {
                        "LEFT" | "TOP" => position * ratio,
                        "RIGHT" | "BOTTOM" => target + (position - reference) * ratio,
                        "CENTER" => (position - reference * 0.5) * ratio + target * 0.5,
                        _ => position,
                    };
                let output_x = aligned(
                    &string("alignH")?,
                    number("posx")?,
                    target_width as f32,
                    reference_width as f32,
                    ratio_x,
                );
                let output_y = aligned(
                    &string("alignV")?,
                    number("posy")?,
                    target_height as f32,
                    reference_height as f32,
                    ratio_y,
                );
                Ok((output_x, output_y, output_scale_x, output_scale_y))
            },
        )?,
    )?;
    globals.set("Align", align)?;
    Ok(())
}
