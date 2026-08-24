//! Scene ownership and live Lua collision-material synchronization.

use super::StellaLua;
use crate::*;

impl StellaLua {
    pub(crate) fn sync_scene_lifetime(&self) -> Result<(), ScriptError> {
        let world = object_world(&self.lua)?;
        let world_identity = world.to_pointer() as usize;
        let mut live_names = world
            .pairs::<Value, Value>()
            .filter_map(|pair| match pair {
                Ok((Value::String(name), Value::Table(_))) => Some(Ok(name.to_string_lossy())),
                Ok(_) => None,
                Err(error) => Some(Err(error)),
            })
            .collect::<LuaResult<BTreeSet<_>>>()?;
        let mut bridge = self.render.lock().expect("render bridge lock poisoned");
        bridge.synchronize_object_world_owner(world_identity);
        let vanished = bridge
            .scene
            .keys()
            .filter(|name| !live_names.contains(*name))
            .cloned()
            .collect::<Vec<_>>();
        let mut propagated = Vec::new();
        for name in vanished {
            propagated.extend(bridge.remove_object_with_destroy_links(&name));
        }
        // Direct Lua table deletion bypassed native removeObject, so there is
        // no live object record against which an EndContact callback can be
        // dispatched. Still expire those Box2D contacts immediately instead
        // of leaking them into the next physics step as delayed exits.
        let vanished_exits = bridge.drain_contacts_for_removed_objects(&propagated);
        let mut clear_inside_gravity = Vec::new();
        for (first, second, sensor) in vanished_exits {
            if sensor {
                clear_inside_gravity.extend(bridge.end_native_sensor_overlap(&first, &second));
            }
        }
        bridge.scene.retain(|name, _| live_names.contains(name));
        bridge.joints.retain(|_, joint| {
            live_names.contains(&joint.first) && live_names.contains(&joint.second)
        });
        bridge
            .tracks
            .retain(|name, track| live_names.contains(name) && live_names.contains(&track.object));
        drop(bridge);
        set_inside_gravity_fields(&self.lua, &clear_inside_gravity, false)?;
        for name in propagated {
            world.raw_set(name.as_str(), Value::Nil)?;
            live_names.remove(&name);
        }
        let mut callbacks = self.draw_callbacks.borrow_mut();
        if callbacks.object_world_identity != Some(world_identity) {
            callbacks.pre.clear();
            callbacks.post.clear();
            callbacks.object_world_identity = Some(world_identity);
        }
        callbacks.pre.retain(|name, _| live_names.contains(name));
        callbacks.post.retain(|name, _| live_names.contains(name));
        Ok(())
    }

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
