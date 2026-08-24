//! Lua 5.1 stack adapter at the head of `sub_100051BBC`.

use crate::*;

#[derive(Debug, Clone)]
pub(super) struct BoxSprites {
    pub(super) top_left: Option<String>,
    pub(super) top_middle: Option<String>,
    pub(super) top_right: Option<String>,
    pub(super) left: Option<String>,
    pub(super) center: Option<String>,
    pub(super) right: Option<String>,
    pub(super) bottom_left: Option<String>,
    pub(super) bottom_middle: Option<String>,
    pub(super) bottom_right: Option<String>,
}

#[derive(Debug, Clone)]
pub(super) struct BoxDrawArguments {
    pub(super) sprites: BoxSprites,
    pub(super) x: f32,
    pub(super) y: f32,
    pub(super) width: f32,
    pub(super) height: f32,
    pub(super) scale_x: f32,
    pub(super) scale_y: f32,
    pub(super) anchor_x: f32,
    pub(super) anchor_y: f32,
    pub(super) background: Option<[f32; 4]>,
}

impl BoxDrawArguments {
    pub(super) fn parse(args: &MultiValue) -> LuaResult<Self> {
        let definition = args.iter().next().and_then(value_table).ok_or_else(|| {
            runtime_error("bad argument #1 to 'drawBoxNative' (table expected)".to_owned())
        })?;
        let number =
            |index| native_required_number(args, index, "drawBoxNative").map(|value| value as f32);
        let x = number(1)?;
        let y = number(2)?;
        let width = number(3)?;
        let height = number(4)?;
        let scale_x = number(5)?;
        let scale_y = number(6)?;
        let horizontal_anchor = native_required_string(args, 7, "drawBoxNative")?;
        let vertical_anchor = native_required_string(args, 8, "drawBoxNative")?;
        let anchor_x = match horizontal_anchor.as_str() {
            "HCENTER" => width * -0.5_f32,
            "RIGHT" => -width,
            _ => 0.0,
        };
        let anchor_y = match vertical_anchor.as_str() {
            "VCENTER" => height * -0.5_f32,
            "BOTTOM" => -height,
            _ => 0.0,
        };
        let background = args
            .iter()
            .nth(9)
            .and_then(value_table)
            .map(|table| color(&table))
            .transpose()?;
        Ok(Self {
            sprites: BoxSprites::parse(&definition)?,
            x,
            y,
            width,
            height,
            scale_x,
            scale_y,
            anchor_x,
            anchor_y,
            background,
        })
    }
}

impl BoxSprites {
    fn parse(definition: &mlua::Table) -> LuaResult<Self> {
        let sprite = |field| {
            definition
                .get::<Value>(field)
                .map(|value| native_lua51_string(&value))
        };
        Ok(Self {
            top_left: sprite("topLeft")?,
            top_middle: sprite("topMiddle")?,
            top_right: sprite("topRight")?,
            left: sprite("left")?,
            center: sprite("center")?,
            right: sprite("right")?,
            bottom_left: sprite("bottomLeft")?,
            bottom_middle: sprite("bottomMiddle")?,
            bottom_right: sprite("bottomRight")?,
        })
    }
}

fn color(table: &mlua::Table) -> LuaResult<[f32; 4]> {
    let channel = |field| {
        table
            .get::<Value>(field)
            .map(|value| native_lua51_number(&value).unwrap_or(1.0) as f32)
    };
    Ok([
        channel("red")?,
        channel("green")?,
        channel("blue")?,
        channel("alpha")?,
    ])
}
