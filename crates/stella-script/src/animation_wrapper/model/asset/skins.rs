//! Companion `.skins.json` resolution and skin transform decoding.

use std::collections::BTreeMap;

use super::super::{AnimationSkin, AnimationSkinTransform};

const ANIMATION_SUFFIX: &[u8] = b".anim.json";
const SKINS_SUFFIX: &[u8] = b".skins.json";

pub(crate) fn animation_skin_filename(filename: &str) -> String {
    // Both native load members use `filename.length() - strlen(".anim.json")`
    // as the std::string substring count without first checking the suffix. An
    // underflowed count is clamped by the substring constructor to the input.
    let bytes = filename.as_bytes();
    let prefix_length = bytes
        .len()
        .checked_sub(ANIMATION_SUFFIX.len())
        .unwrap_or(bytes.len());
    let mut result = Vec::with_capacity(prefix_length + SKINS_SUFFIX.len());
    result.extend_from_slice(&bytes[..prefix_length]);
    result.extend_from_slice(SKINS_SUFFIX);
    String::from_utf8_lossy(&result).into_owned()
}

pub(crate) fn parse_animation_skins(
    document: &serde_json::Value,
) -> BTreeMap<String, AnimationSkin> {
    let mut skins = BTreeMap::new();
    let Some(skin_sets) = document.as_object() else {
        return skins;
    };
    for (skin_name, slots) in skin_sets {
        let Some(slots) = slots.as_object() else {
            continue;
        };
        let mut skin = AnimationSkin::new();
        for (slot_name, sprites) in slots {
            let Some(sprites) = sprites.as_object() else {
                continue;
            };
            let mut variants = BTreeMap::new();
            for (sprite_name, value) in sprites {
                let Some(properties) = value.as_object() else {
                    continue;
                };
                variants.insert(
                    sprite_name.clone(),
                    AnimationSkinTransform {
                        sprite: properties
                            .get("name")
                            .and_then(serde_json::Value::as_str)
                            .unwrap_or(sprite_name)
                            .rsplit('/')
                            .next()
                            .unwrap_or(sprite_name)
                            .to_owned(),
                        x: properties
                            .get("x")
                            .and_then(serde_json::Value::as_f64)
                            .map(|value| f64::from(value as f32))
                            .unwrap_or(0.0),
                        y: properties
                            .get("y")
                            .and_then(serde_json::Value::as_f64)
                            .map(|value| f64::from(value as f32))
                            .unwrap_or(0.0),
                        scale_x: properties
                            .get("scaleX")
                            .and_then(serde_json::Value::as_f64)
                            .map(|value| f64::from(value as f32))
                            .unwrap_or(1.0),
                        scale_y: properties
                            .get("scaleY")
                            .and_then(serde_json::Value::as_f64)
                            .map(|value| f64::from(value as f32))
                            .unwrap_or(1.0),
                        angle: properties
                            .get("rotation")
                            .and_then(serde_json::Value::as_f64)
                            .map(|value| f64::from(value as f32))
                            .unwrap_or(0.0),
                    },
                );
            }
            skin.insert(slot_name.clone(), variants);
        }
        skins.insert(skin_name.clone(), skin);
    }
    skins
}

#[cfg(test)]
mod tests {
    use super::animation_skin_filename;

    #[test]
    fn companion_filename_uses_native_fixed_length_replacement() {
        assert_eq!(
            animation_skin_filename("animations/stella.anim.json"),
            "animations/stella.skins.json"
        );
        assert_eq!(animation_skin_filename("abcdefghijk"), "a.skins.json");
        assert_eq!(animation_skin_filename("short"), "short.skins.json");
    }
}
