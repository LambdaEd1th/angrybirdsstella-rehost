//! Animation JSON loading and hierarchy recovery.

use std::{collections::BTreeMap, fs, path::Path};

use super::super::*;
#[cfg(test)]
use super::skins::animation_skin_filename;
use super::skins::parse_animation_skins;

pub(crate) fn read_animation_bytes(data_root: &Path, filename: &str) -> mlua::Result<Vec<u8>> {
    let path = data_root.join(filename.trim_start_matches('/'));
    fs::read(&path).map_err(|error| {
        crate::runtime_error(format!(
            "Animation JSON '{}' could not be read: {error}",
            path.display()
        ))
    })
}

pub(crate) fn parse_animation_document(
    bytes: &[u8],
    filename: &str,
) -> mlua::Result<serde_json::Value> {
    // util::JSON accepts an empty stream as its default/null value, but a
    // nonempty malformed document throws before AnimationWrapper::load.
    let document = if bytes.is_empty() {
        serde_json::Value::Null
    } else {
        serde_json::from_slice::<serde_json::Value>(bytes).map_err(|error| {
            crate::runtime_error(format!("Parsing JSON failed: {filename}: {error}"))
        })?
    };
    Ok(document)
}

pub(crate) fn animation_asset_from_documents(
    document: &serde_json::Value,
    skin_document: &serde_json::Value,
) -> AnimationAsset {
    let skins = parse_animation_skins(skin_document);
    let mut durations = BTreeMap::new();
    let Some(components) = document.get("comps").and_then(serde_json::Value::as_array) else {
        return AnimationAsset {
            actions: durations,
            definition: AnimationDefinition {
                skins,
                ..AnimationDefinition::default()
            },
        };
    };
    for component in components {
        let Some(actions) = component
            .get("data")
            .and_then(|value| value.get("actions"))
            .and_then(serde_json::Value::as_object)
        else {
            continue;
        };
        for (name, action) in actions {
            let mut duration = 0.0_f64;
            collect_animation_keyframe_time(action, &mut duration);
            durations
                .entry(name.clone())
                .and_modify(|known| *known = known.max(duration))
                .or_insert(duration);
        }
    }
    let mut definition = AnimationDefinition::default();
    if let Some(children) = document
        .get("children")
        .and_then(serde_json::Value::as_array)
    {
        for child in children {
            collect_animation_nodes(child, None, &mut definition);
        }
    }
    for component in components {
        let Some(actions) = component
            .get("data")
            .and_then(|value| value.get("actions"))
            .and_then(serde_json::Value::as_object)
        else {
            continue;
        };
        for (name, value) in actions {
            parse_animation_action(value, definition.actions.entry(name.clone()).or_default());
        }
    }
    definition.skins = skins;
    AnimationAsset {
        actions: durations,
        definition,
    }
}

#[cfg(test)]
pub(crate) fn animation_asset(data_root: &Path, filename: &str) -> mlua::Result<AnimationAsset> {
    let document = parse_animation_document(&read_animation_bytes(data_root, filename)?, filename)?;
    let skin_filename = animation_skin_filename(filename);
    let skins = parse_animation_document(
        &read_animation_bytes(data_root, &skin_filename)?,
        &skin_filename,
    )?;
    Ok(animation_asset_from_documents(&document, &skins))
}

fn collect_animation_keyframe_time(value: &serde_json::Value, maximum: &mut f64) {
    match value {
        serde_json::Value::Object(object) => {
            if let Some(keyframes) = object
                .get("keyframes")
                .and_then(serde_json::Value::as_array)
            {
                for keyframe in keyframes {
                    if let Some(time) = keyframe
                        .as_array()
                        .and_then(|parts| parts.first())
                        .and_then(serde_json::Value::as_f64)
                    {
                        *maximum = maximum.max(time);
                    }
                }
            }
            for child in object.values() {
                collect_animation_keyframe_time(child, maximum);
            }
        }
        serde_json::Value::Array(array) => {
            for child in array {
                collect_animation_keyframe_time(child, maximum);
            }
        }
        _ => {}
    }
}

fn collect_animation_nodes(
    node: &serde_json::Value,
    parent: Option<&str>,
    definition: &mut AnimationDefinition,
) {
    let Some(name) = node.get("name").and_then(serde_json::Value::as_str) else {
        return;
    };
    definition.entities.insert(name.to_owned());
    if let Some(parent) = parent {
        definition
            .parents
            .insert(name.to_owned(), parent.to_owned());
    }
    let is_sprite_slot = node
        .get("comps")
        .and_then(serde_json::Value::as_array)
        .is_some_and(|components| {
            components.iter().any(|component| {
                component
                    .get("type")
                    .and_then(serde_json::Value::as_str)
                    .is_some_and(|kind| kind.contains("SpriteComponent"))
            })
        });
    if is_sprite_slot {
        definition.slots.push(name.to_owned());
    }
    if let Some(children) = node.get("children").and_then(serde_json::Value::as_array) {
        for child in children {
            collect_animation_nodes(child, Some(name), definition);
        }
    }
}
