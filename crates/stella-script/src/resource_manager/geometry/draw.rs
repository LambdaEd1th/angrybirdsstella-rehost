//! Native drawSprite overload parsing and anchor resolution.

use mlua::{MultiValue, Result as LuaResult, Value};

use crate::{runtime_error, value_number, value_string};

use super::model::SpriteGeometry;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SpriteHorizontalAnchor {
    Left,
    Center,
    Right,
    Pivot,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SpriteVerticalAnchor {
    Top,
    Center,
    Bottom,
    Baseline,
    Pivot,
}

pub(crate) struct ParsedSpriteDraw {
    pub(crate) sprite: String,
    pub(crate) x: f64,
    pub(crate) y: f64,
    pub(crate) horizontal_anchor: SpriteHorizontalAnchor,
    pub(crate) vertical_anchor: SpriteVerticalAnchor,
    pub(crate) draw_size: Option<[f64; 2]>,
}

pub(crate) fn parse_draw_sprite_args(args: &MultiValue) -> LuaResult<Option<ParsedSpriteDraw>> {
    let values = args.iter().collect::<Vec<_>>();
    // sub_1004483AC distinguishes dot and colon calls solely by checking
    // whether Lua argument 2 is numeric. It then strictly consumes the
    // corresponding string/x/y triplet; it does not default missing
    // coordinates or recover by searching the argument list.
    let sprite_index = if matches!(values.get(1), Some(Value::Integer(_) | Value::Number(_))) {
        0
    } else {
        1
    };
    let sprite = values
        .get(sprite_index)
        .and_then(|value| value_string(value))
        .ok_or_else(|| {
            runtime_error(format!(
                "drawSprite argument {} must be string",
                sprite_index + 1
            ))
        })?;
    let x = values
        .get(sprite_index + 1)
        .and_then(|value| value_number(value))
        .ok_or_else(|| {
            runtime_error(format!(
                "drawSprite argument {} must be number",
                sprite_index + 2
            ))
        })?;
    let y = values
        .get(sprite_index + 2)
        .and_then(|value| value_number(value))
        .ok_or_else(|| {
            runtime_error(format!(
                "drawSprite argument {} must be number",
                sprite_index + 3
            ))
        })?;
    let mut horizontal_anchor = SpriteHorizontalAnchor::Pivot;
    let mut vertical_anchor = SpriteVerticalAnchor::Pivot;
    for (index, value) in values.iter().enumerate().skip(sprite_index + 3).take(2) {
        let anchor = value_string(value).ok_or_else(|| {
            runtime_error(format!("drawSprite argument {} must be string", index + 1))
        })?;
        if anchor.is_empty() {
            continue;
        }
        match anchor.as_str() {
            "TOP" => vertical_anchor = SpriteVerticalAnchor::Top,
            "VCENTER" => vertical_anchor = SpriteVerticalAnchor::Center,
            "BOTTOM" => vertical_anchor = SpriteVerticalAnchor::Bottom,
            "BASELINE" => vertical_anchor = SpriteVerticalAnchor::Baseline,
            "VPIVOT" => vertical_anchor = SpriteVerticalAnchor::Pivot,
            "LEFT" => horizontal_anchor = SpriteHorizontalAnchor::Left,
            "HCENTER" => horizontal_anchor = SpriteHorizontalAnchor::Center,
            "RIGHT" => horizontal_anchor = SpriteHorizontalAnchor::Right,
            "HPIVOT" => horizontal_anchor = SpriteHorizontalAnchor::Pivot,
            _ => return Err(runtime_error(format!("Invalid anchor: {anchor}"))),
        }
    }
    let draw_size = if values.len() >= sprite_index + 7 {
        Some([
            values
                .get(sprite_index + 5)
                .and_then(|value| value_number(value))
                .ok_or_else(|| {
                    runtime_error(format!(
                        "drawSprite argument {} must be number",
                        sprite_index + 6
                    ))
                })?,
            values
                .get(sprite_index + 6)
                .and_then(|value| value_number(value))
                .ok_or_else(|| {
                    runtime_error(format!(
                        "drawSprite argument {} must be number",
                        sprite_index + 7
                    ))
                })?,
        ])
    } else {
        None
    };
    if sprite.is_empty() {
        return Ok(None);
    }
    Ok(Some(ParsedSpriteDraw {
        sprite,
        x,
        y,
        horizontal_anchor,
        vertical_anchor,
        draw_size,
    }))
}

pub(crate) fn sprite_draw_anchor_offset_from_geometry(
    geometry: SpriteGeometry,
    horizontal_anchor: SpriteHorizontalAnchor,
    vertical_anchor: SpriteVerticalAnchor,
) -> (f64, f64) {
    let x = match horizontal_anchor {
        SpriteHorizontalAnchor::Left => -geometry.min_x,
        SpriteHorizontalAnchor::Center => -geometry.min_x - geometry.width() * 0.5,
        SpriteHorizontalAnchor::Right => -geometry.max_x,
        SpriteHorizontalAnchor::Pivot => 0.0,
    };
    let y = match vertical_anchor {
        SpriteVerticalAnchor::Top => -geometry.min_y,
        SpriteVerticalAnchor::Center => -geometry.min_y - geometry.height() * 0.5,
        SpriteVerticalAnchor::Bottom => -geometry.max_y,
        SpriteVerticalAnchor::Baseline | SpriteVerticalAnchor::Pivot => 0.0,
    };
    (x, y)
}
