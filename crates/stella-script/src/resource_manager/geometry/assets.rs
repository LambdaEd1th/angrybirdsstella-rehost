//! KA3D sprite/composite loading and recursive runtime bounds.

use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::Path,
};

use stella_assets::ka3d::{CompositePart, CompositeSpriteSet, Ka3dEnvelope, SpriteSheet};

use super::model::SpriteGeometry;

pub(crate) type LoadedSpriteGeometry = (
    BTreeMap<String, SpriteGeometry>,
    BTreeSet<String>,
    BTreeMap<String, Vec<CompositePart>>,
);

pub(crate) fn load_sprite_geometry(data_root: &Path) -> LoadedSpriteGeometry {
    let image_root = data_root.join("images/1024x768");
    let mut paths = match fs::read_dir(image_root) {
        Ok(entries) => entries
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| path.extension().and_then(|value| value.to_str()) == Some("dat"))
            .collect::<Vec<_>>(),
        Err(_) => return (BTreeMap::new(), BTreeSet::new(), BTreeMap::new()),
    };
    paths.sort();
    let mut geometry = BTreeMap::new();
    let mut composites = BTreeMap::new();
    for path in paths {
        let Ok(bytes) = fs::read(path) else {
            continue;
        };
        if Ka3dEnvelope::find(&bytes, b"SPRT").is_ok() {
            let Ok(sheet) = SpriteSheet::parse(&bytes) else {
                continue;
            };
            for sprite in sheet.sprites {
                geometry.insert(
                    sprite.name,
                    SpriteGeometry {
                        min_x: -f64::from(sprite.pivot_x),
                        min_y: -f64::from(sprite.pivot_y),
                        max_x: f64::from(sprite.width) - f64::from(sprite.pivot_x),
                        max_y: f64::from(sprite.height) - f64::from(sprite.pivot_y),
                    },
                );
            }
        } else if Ka3dEnvelope::find(&bytes, b"COMP").is_ok() {
            let Ok(set) = CompositeSpriteSet::parse(&bytes) else {
                continue;
            };
            for sprite in set.sprites {
                composites.insert(sprite.name, sprite.parts);
            }
        }
    }

    let names = composites.keys().cloned().collect::<Vec<_>>();
    for name in names {
        resolve_composite_geometry(&name, &composites, &mut geometry, &mut BTreeSet::new());
    }
    if std::env::var_os("STELLA_TRACE_SPRITE_GEOMETRY").is_some() {
        eprintln!(
            "sprite geometry: {} resolved, {} composites; BTN_OPTIONS_SMALL={:?}",
            geometry.len(),
            composites.len(),
            geometry.get("BTN_OPTIONS_SMALL")
        );
        if let Some(parts) = composites.get("BTN_OPTIONS_SMALL") {
            for part in parts {
                eprintln!("  part {:?}: {:?}", part.sprite, geometry.get(&part.sprite));
            }
        }
    }
    let composite_names = composites.keys().cloned().collect();
    (geometry, composite_names, composites)
}

fn resolve_composite_geometry(
    name: &str,
    composites: &BTreeMap<String, Vec<CompositePart>>,
    geometry: &mut BTreeMap<String, SpriteGeometry>,
    visiting: &mut BTreeSet<String>,
) -> Option<SpriteGeometry> {
    if let Some(result) = geometry.get(name).copied() {
        return Some(result);
    }
    if !visiting.insert(name.to_owned()) {
        return None;
    }
    let parts = composites.get(name)?;
    let mut result: Option<SpriteGeometry> = None;
    for part in parts {
        let atlas_name = part
            .sprite
            .split_once('#')
            .map_or(part.sprite.as_str(), |(base, _)| base);
        let child = resolve_composite_geometry(atlas_name, composites, geometry, visiting)?;
        let child = child.transformed(part);
        match &mut result {
            Some(current) => current.include(child),
            None => result = Some(child),
        }
    }
    visiting.remove(name);
    if let Some(result) = result {
        geometry.insert(name.to_owned(), result);
    }
    result
}
