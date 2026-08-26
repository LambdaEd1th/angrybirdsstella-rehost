//! Live Lua collision-material synchronization.

use super::StellaLua;
use crate::*;

impl StellaLua {
    pub(crate) fn sync_native_collision_filter_state(&self) -> Result<(), ScriptError> {
        let world = object_world(&self.lua)?;
        let names = self
            .render
            .lock()
            .expect("render bridge lock poisoned")
            .scene
            .keys()
            .cloned()
            .collect::<Vec<_>>();
        let mut updates = Vec::with_capacity(names.len());
        for name in names {
            let Value::Table(entry) = world.raw_get::<Value>(name.as_str())? else {
                continue;
            };
            let material = entry
                .get::<Value>("material")
                .ok()
                .as_ref()
                .and_then(value_string);
            let collision_materials = match entry.get::<Value>("collisionMaterials") {
                Ok(Value::Table(materials)) => {
                    let mut result = Vec::with_capacity(materials.raw_len());
                    for index in 1..=materials.raw_len() {
                        if let Ok(value) = materials.raw_get::<Value>(index)
                            && let Some(material) = value_string(&value)
                        {
                            result.push(material);
                        }
                    }
                    result
                }
                _ => Vec::new(),
            };
            updates.push((name, material, collision_materials));
        }
        let mut bridge = self.render.lock().expect("render bridge lock poisoned");
        for (name, material, collision_materials) in updates {
            if let Some(object) = bridge.scene.get_mut(&name) {
                if let Some(material) = material {
                    object.material = material;
                }
                object.collision_materials = collision_materials;
            }
        }
        Ok(())
    }
}
