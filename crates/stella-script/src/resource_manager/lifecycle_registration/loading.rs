//! File-loader dispatch owned by Purple's SpriteSheet, CompoSpriteSet,
//! BitmapFont and TextGroupSet constructors.

use std::{fs, path::Path};

use mlua::Result as LuaResult;
use serde_json::{Map, Value};
use stella_assets::ka3d::{
    BitmapFont, CompositePart, CompositeSprite, CompositeSpriteSet, LocalizationTable,
    SpriteRegion, SpriteSheet,
};

use crate::{native_fcvtzs_f32, resolve_data_file, resource_file_extension, runtime_error};

pub(crate) fn load_sprite_sheet_path(path: &Path, source: &str) -> LuaResult<SpriteSheet> {
    let bytes = fs::read(path).map_err(runtime_error)?;
    match resource_file_extension(source).as_str() {
        ".dat" => SpriteSheet::parse(&bytes).map_err(runtime_error),
        ".json" => parse_json_sprite_sheet(&parse_json(&bytes)?),
        extension => Err(unsupported_extension("SpriteSheet", extension)),
    }
}

pub(super) fn load_composite_set_source(
    data_root: &Path,
    source: &str,
) -> LuaResult<Option<CompositeSpriteSet>> {
    let bytes = read_resource(data_root, source)?;
    match resource_file_extension(source).as_str() {
        ".dat" => CompositeSpriteSet::parse(&bytes)
            .map(|set| (!set.sprites.is_empty()).then_some(set))
            .map_err(runtime_error),
        ".json" => parse_json_composite_set(&parse_json(&bytes)?),
        extension => Err(unsupported_extension("CompoSpriteSet", extension)),
    }
}

pub(super) fn load_bitmap_font_source(data_root: &Path, source: &str) -> LuaResult<BitmapFont> {
    BitmapFont::parse(&read_resource(data_root, source)?).map_err(runtime_error)
}

pub(super) fn load_text_group_set_source(
    data_root: &Path,
    source: &str,
) -> LuaResult<LocalizationTable> {
    LocalizationTable::parse(&read_resource(data_root, source)?).map_err(runtime_error)
}

fn read_resource(data_root: &Path, source: &str) -> LuaResult<Vec<u8>> {
    let path = resolve_data_file(data_root, source).map_err(runtime_error)?;
    fs::read(path).map_err(runtime_error)
}

fn parse_json(bytes: &[u8]) -> LuaResult<Value> {
    serde_json::from_slice(bytes).map_err(runtime_error)
}

fn unsupported_extension(kind: &str, extension: &str) -> mlua::Error {
    // The iOS build leaves the loader pointer null and faults at its virtual
    // call for this branch. Crossing that ABI boundary as a catchable Lua
    // error preserves the failure without terminating the Rust host process.
    runtime_error(format!("Unsupported {kind} file extension: {extension:?}"))
}

fn required_object<'a>(value: &'a Value, field: &str) -> LuaResult<&'a Map<String, Value>> {
    value
        .as_object()
        .ok_or_else(|| invalid_json_field(field, "object"))
}

fn required_array<'a>(value: &'a Value, field: &str) -> LuaResult<&'a Vec<Value>> {
    value
        .as_array()
        .ok_or_else(|| invalid_json_field(field, "array"))
}

fn required_string<'a>(object: &'a Map<String, Value>, field: &str) -> LuaResult<&'a str> {
    object
        .get(field)
        .and_then(Value::as_str)
        .ok_or_else(|| invalid_json_field(field, "string"))
}

fn required_number(object: &Map<String, Value>, field: &str) -> LuaResult<f64> {
    object
        .get(field)
        .and_then(Value::as_f64)
        .ok_or_else(|| invalid_json_field(field, "number"))
}

fn required_native_integer(object: &Map<String, Value>, field: &str) -> LuaResult<i32> {
    let number = object
        .get(field)
        .and_then(Value::as_number)
        .ok_or_else(|| invalid_json_field(field, "number"))?;
    // sub_10055AE64 preserves parser integers as signed i64. Parser doubles
    // use FCVTZS X8,D0: signed saturation and NaN-to-zero, not x86's
    // integer-indefinite result. Preserve the 64-bit conversion before
    // narrowing: positive overflow therefore has a low signed word of -1.
    // sub_10055D93C then exposes only the low W word at map-node +0xa0.
    if let Some(value) = number.as_i64() {
        return Ok(value as i32);
    }
    if number.as_u64().is_some() {
        return Err(invalid_json_field(field, "signed 64-bit number"));
    }
    let value = number
        .as_f64()
        .ok_or_else(|| invalid_json_field(field, "number"))?;
    Ok(native_json_double_integer_view(value))
}

fn native_json_double_integer_view(value: f64) -> i32 {
    (value as i64) as i32
}

fn optional_number(object: &Map<String, Value>, field: &str) -> LuaResult<Option<f64>> {
    object
        .get(field)
        .map(|value| {
            value
                .as_f64()
                .ok_or_else(|| invalid_json_field(field, "number"))
        })
        .transpose()
}

fn invalid_json_field(field: &str, expected: &str) -> mlua::Error {
    runtime_error(format!("JSON field {field:?} must be a {expected}"))
}

fn metadata_application(root: &Value) -> LuaResult<&str> {
    let root = required_object(root, "root")?;
    let metadata = root
        .get("meta")
        .ok_or_else(|| invalid_json_field("meta", "object"))?;
    required_string(required_object(metadata, "meta")?, "app")
}

fn parse_json_sprite_sheet(root: &Value) -> LuaResult<SpriteSheet> {
    let app = metadata_application(root)?;
    let root_object = required_object(root, "root")?;
    let metadata = required_object(
        root_object
            .get("meta")
            .ok_or_else(|| invalid_json_field("meta", "object"))?,
        "meta",
    )?;
    let image = metadata
        .get("image")
        .map(|value| {
            value
                .as_str()
                .ok_or_else(|| invalid_json_field("image", "string"))
        })
        .transpose()?
        .unwrap_or("");
    let frames = root_object
        .get("frames")
        .ok_or_else(|| invalid_json_field("frames", "array"))?;

    let mut sprites = Vec::new();
    if app == "http://www.texturepacker.com" {
        let Some(frames) = frames.as_array() else {
            return Err(runtime_error(
                "Unsupported TexturePacker JSON sheet format (use JSON Array format instead)",
            ));
        };
        for frame in frames {
            sprites.push(parse_json_sprite_frame(frame, false)?);
        }
    } else {
        if !app.contains("Adobe") && !app.contains("ArtPacker") {
            return Err(runtime_error("Unsupported JSON sheet format"));
        }
        for frame in required_array(frames, "frames")? {
            sprites.push(parse_json_sprite_frame(frame, true)?);
        }
    }
    let sprite_texture_indices = vec![0; sprites.len()];
    Ok(SpriteSheet {
        textures: (!image.is_empty())
            .then(|| image.to_owned())
            .into_iter()
            .collect(),
        sprites,
        sprite_texture_indices,
    })
}

fn parse_json_sprite_frame(value: &Value, adobe_family: bool) -> LuaResult<SpriteRegion> {
    let entry = required_object(value, "frame entry")?;
    let name = required_string(entry, "filename")?.to_owned();
    let frame = entry
        .get("frame")
        .ok_or_else(|| invalid_json_field("frame", "object"))?;
    let frame = required_object(frame, "frame")?;
    // JSON geometry is read through the native 32-bit integer accessor. The
    // Sprite constructor calculates defaults first, then STRH-truncates all
    // six public geometry members.
    let native_x = required_native_integer(frame, "x")?;
    let native_y = required_native_integer(frame, "y")?;
    let native_width = required_native_integer(frame, "w")?;
    let native_height = required_native_integer(frame, "h")?;

    let (pivot_x, pivot_y) = if adobe_family && let Some(pivot) = entry.get("pivot") {
        let pivot = required_object(pivot, "pivot")?;
        (
            native_fcvtzs_f32((required_number(pivot, "x")? as f32 + 0.5).floor()),
            native_fcvtzs_f32((required_number(pivot, "y")? as f32 + 0.5).floor()),
        )
    } else {
        (native_width / 2, native_height / 2)
    };
    let atlas_rotation = if adobe_family {
        0
    } else {
        entry
            .get("rotated")
            .and_then(Value::as_bool)
            .ok_or_else(|| invalid_json_field("rotated", "boolean"))? as u8
    };
    Ok(SpriteRegion {
        name,
        x: native_x as i16,
        y: native_y as i16,
        width: native_width as i16,
        height: native_height as i16,
        pivot_x: pivot_x as i16,
        pivot_y: pivot_y as i16,
        atlas_rotation,
    })
}

fn parse_json_composite_set(root: &Value) -> LuaResult<Option<CompositeSpriteSet>> {
    let app = metadata_application(root)?;
    if !app.contains("Adobe") && !app.contains("ArtPacker") {
        return Err(runtime_error("Unsupported JSON composprite format"));
    }
    let root = required_object(root, "root")?;
    let Some(composites) = root.get("compo") else {
        return Ok(None);
    };
    let composites = required_array(composites, "compo")?;
    let mut parsed = Vec::with_capacity(composites.len());
    for composite in composites {
        let composite = required_object(composite, "compo entry")?;
        let name = required_string(composite, "name")?.to_owned();
        let sprites = composite
            .get("sprites")
            .ok_or_else(|| invalid_json_field("sprites", "array"))?;
        let parts = required_array(sprites, "sprites")?
            .iter()
            .rev()
            .map(parse_json_composite_part)
            .collect::<LuaResult<Vec<_>>>()?;
        parsed.push(CompositeSprite { name, parts });
    }
    Ok((!parsed.is_empty()).then_some(CompositeSpriteSet { sprites: parsed }))
}

fn parse_json_composite_part(value: &Value) -> LuaResult<CompositePart> {
    let part = required_object(value, "sprite entry")?;
    let atlas_sprite = required_string(part, "name")?;
    // hasString("id") ignores an absent or non-string member. A non-empty id
    // is retained in the Entry map but is not part of the atlas lookup.
    let id = part.get("id").and_then(Value::as_str).unwrap_or("");
    let sprite = if id.is_empty() {
        atlas_sprite.to_owned()
    } else {
        format!("{atlas_sprite}#{id}")
    };
    let x = required_number(part, "x")? as f32;
    let y = required_number(part, "y")? as f32;
    let angle =
        optional_number(part, "angle")?.unwrap_or(0.0) as f32 * (std::f32::consts::PI / 180.0);

    let (scale_x, scale_y) = if let Some(scale) = part.get("scale") {
        if let Some(values) = scale.as_array() {
            let x = values
                .first()
                .and_then(Value::as_f64)
                .ok_or_else(|| invalid_json_field("scale", "two-number array"))?;
            let y = values
                .get(1)
                .and_then(Value::as_f64)
                .ok_or_else(|| invalid_json_field("scale", "two-number array"))?;
            (x as f32, y as f32)
        } else if let Some(scale) = scale.as_f64() {
            (scale as f32, scale as f32)
        } else if scale.as_f64().is_none() {
            return Err(invalid_json_field("scale", "number or number array"));
        } else {
            unreachable!()
        }
    } else {
        (1.0, 1.0)
    };
    let (flip_x, flip_y) = match part.get("flip") {
        None => (1.0, 1.0),
        Some(value) => {
            let values = required_array(value, "flip")?;
            let x = values
                .first()
                .and_then(Value::as_f64)
                .ok_or_else(|| invalid_json_field("flip", "two-number array"))?;
            let y = values
                .get(1)
                .and_then(Value::as_f64)
                .ok_or_else(|| invalid_json_field("flip", "two-number array"))?;
            (x as f32, y as f32)
        }
    };
    Ok(CompositePart {
        sprite,
        x,
        y,
        scale_x,
        scale_y,
        flip_x,
        flip_y,
        angle,
        visible: true,
    })
}

#[cfg(test)]
mod tests {
    use super::{
        native_json_double_integer_view, parse_json_composite_set, parse_json_sprite_sheet,
    };

    #[test]
    fn json_double_integer_view_saturates_to_i64_before_low_word_narrowing() {
        for (value, expected) in [
            (f64::NAN, 0),
            (f64::INFINITY, -1),
            (f64::NEG_INFINITY, 0),
            (9_223_372_036_854_775_808.0, -1),
            (-9_223_372_036_854_775_808.0, 0),
            (f64::MAX, -1),
            (-f64::MAX, 0),
            (4_294_967_295.75, -1),
            (4_294_967_296.75, 0),
            (-4_294_967_297.75, -1),
        ] {
            assert_eq!(
                native_json_double_integer_view(value),
                expected,
                "{value:?}"
            );
        }

        // Exercise the actual JSON-number accessor and Sprite STRH boundary,
        // not just the scalar helper. Finite JSON doubles may exceed i64.
        let source = serde_json::json!({
            "meta": {"app": "http://www.texturepacker.com", "image": "atlas.png"},
            "frames": [{
                "filename": "SATURATED",
                "frame": {"x": 1e100, "y": -1e100, "w": 1e100, "h": 4},
                "rotated": false
            }]
        });
        let sheet = parse_json_sprite_sheet(&source).unwrap();
        let sprite = &sheet.sprites[0];
        assert_eq!(
            (sprite.x, sprite.y, sprite.width, sprite.height),
            (-1, 0, -1, 4)
        );
        assert_eq!((sprite.pivot_x, sprite.pivot_y), (0, 2));
    }

    #[cfg(target_arch = "aarch64")]
    #[test]
    fn json_double_integer_view_matches_actual_arm64_fcvtzs_x_then_low_word() {
        let mut bits = 0x9862_3589_932A_B430u64;
        for fixed in [
            0,
            0x7FF0_0000_0000_0000,
            0xFFF0_0000_0000_0000,
            0x7FF8_0000_0000_0000,
            0x43E0_0000_0000_0000,
            0xC3E0_0000_0000_0000,
        ] {
            for index in 0..1024 {
                bits = bits
                    .wrapping_mul(6_364_136_223_846_793_005)
                    .wrapping_add(1_442_695_040_888_963_407);
                let value = f64::from_bits(if index == 0 { fixed } else { bits });
                let native: i64;
                // SAFETY: register-only FCVTZS X,D from Purple's JSON
                // conversion, with no stack or memory access.
                unsafe {
                    std::arch::asm!(
                        "fcvtzs {result:x}, {value:d}",
                        result = out(reg) native,
                        value = in(vreg) value,
                        options(nomem, nostack),
                    );
                }
                assert_eq!(
                    native_json_double_integer_view(value),
                    native as i32,
                    "bits={:016x}",
                    value.to_bits()
                );
            }
        }
    }

    #[test]
    fn json_loader_families_and_empty_composites_match_native_dispatch() {
        let texture_packer = serde_json::json!({
            "meta": {"app": "http://www.texturepacker.com", "image": "atlas.png"},
            "frames": [{
                "filename": "BIRD",
                "frame": {"x": -3, "y": -5, "w": -7, "h": -9},
                "rotated": true
            }]
        });
        let texture_packer = parse_json_sprite_sheet(&texture_packer).unwrap();
        let sprite = &texture_packer.sprites[0];
        assert_eq!((sprite.x, sprite.y), (-3, -5));
        assert_eq!((sprite.width, sprite.height), (-7, -9));
        assert_eq!((sprite.pivot_x, sprite.pivot_y), (-3, -4));
        assert_eq!(sprite.atlas_rotation, 1);

        let missing_rotation = serde_json::json!({
            "meta": {"app": "http://www.texturepacker.com"},
            "frames": [{
                "filename": "BIRD",
                "frame": {"x": 1, "y": 2, "w": 3, "h": 4}
            }]
        });
        assert!(
            parse_json_sprite_sheet(&missing_rotation)
                .unwrap_err()
                .to_string()
                .contains("rotated")
        );

        let object_frames = serde_json::json!({
            "meta": {"app": "http://www.texturepacker.com"},
            "frames": {"BIRD": {}}
        });
        assert!(
            parse_json_sprite_sheet(&object_frames)
                .unwrap_err()
                .to_string()
                .contains("use JSON Array format instead")
        );

        let adobe = serde_json::json!({
            "meta": {"app": "Adobe Animate"},
            "frames": [{
                "filename": "BIRD",
                "frame": {"x": -1, "y": -2, "w": 3, "h": 4},
                "pivot": {"x": -0.75, "y": 0.75}
            }]
        });
        let adobe = parse_json_sprite_sheet(&adobe).unwrap();
        assert_eq!((adobe.sprites[0].x, adobe.sprites[0].y), (-1, -2));
        assert_eq!(
            (adobe.sprites[0].pivot_x, adobe.sprites[0].pivot_y),
            (-1, 1)
        );
        assert_eq!(adobe.sprites[0].atlas_rotation, 0);

        let wide_double_integer_view = serde_json::json!({
            "meta": {"app": "Adobe Animate"},
            "frames": [{
                "filename": "WIDE",
                "frame": {"x": 4294967295.0, "y": 4294967296.0, "w": 3, "h": 4}
            }]
        });
        let wide_double_integer_view = parse_json_sprite_sheet(&wide_double_integer_view).unwrap();
        assert_eq!(
            (
                wide_double_integer_view.sprites[0].x,
                wide_double_integer_view.sprites[0].y,
            ),
            (-1, 0)
        );

        let empty = serde_json::json!({"meta": {"app": "ArtPacker"}, "compo": []});
        assert!(parse_json_composite_set(&empty).unwrap().is_none());
        let nonempty = serde_json::json!({
            "meta": {"app": "Adobe Animate"},
            "compo": [{
                "name": "BUTTON",
                "sprites": [
                    {
                        "name": "PART_A", "id": "front", "x": 1.25, "y": -2.5,
                        "scale": [2.0, 3.0], "flip": [-0.5, 0.25], "angle": 90.0
                    },
                    {
                        "name": "PART_B", "id": 7, "x": 4.0, "y": 5.0,
                        "scale": 1.5
                    }
                ]
            }]
        });
        let nonempty = parse_json_composite_set(&nonempty).unwrap().unwrap();
        let parts = &nonempty.sprites[0].parts;
        // SheetLoaderJSON iterates the JSON array from the final element to
        // the first. A non-string id is ignored by hasString("id").
        assert_eq!(parts[0].sprite, "PART_B");
        assert_eq!((parts[0].scale_x, parts[0].scale_y), (1.5, 1.5));
        assert_eq!((parts[0].flip_x, parts[0].flip_y), (1.0, 1.0));
        assert_eq!(parts[0].angle, 0.0);
        assert_eq!(parts[1].sprite, "PART_A#front");
        assert_eq!((parts[1].x, parts[1].y), (1.25, -2.5));
        assert_eq!((parts[1].scale_x, parts[1].scale_y), (2.0, 3.0));
        assert_eq!((parts[1].flip_x, parts[1].flip_y), (-0.5, 0.25));
        assert_eq!(
            parts[1].angle.to_bits(),
            (90.0_f32 * (std::f32::consts::PI / 180.0)).to_bits()
        );

        let scalar_flip = serde_json::json!({
            "meta": {"app": "ArtPacker"},
            "compo": [{"name": "BAD", "sprites": [
                {"name": "PART", "x": 0, "y": 0, "flip": true}
            ]}]
        });
        assert!(
            parse_json_composite_set(&scalar_flip)
                .unwrap_err()
                .to_string()
                .contains("flip")
        );
    }
}
