//! Persistent `z -> SpriteSheet* -> vector<string>` index at GameLua+0x310.
//!
//! The two `operator[]` helpers are `sub_100073110` and `sub_100073220`.
//! Purple deliberately retains empty tree nodes when a name leaves a leaf.

use std::collections::BTreeMap;
use std::ops::Bound::{Excluded, Included, Unbounded};
use std::sync::Arc;

use crate::SceneObject;

#[derive(Debug, Default)]
pub(crate) struct NativeSceneRenderIndex {
    /// Purple's iOS libstdc++ stores an eight-byte COW `std::string` handle in
    /// each leaf vector. `Arc<str>` preserves that pointer-cheap copy contract
    /// while callbacks force us to release the bridge lock between entries.
    z_buckets: BTreeMap<i32, BTreeMap<u64, Vec<Arc<str>>>>,
}

impl NativeSceneRenderIndex {
    pub(crate) fn append(&mut self, z: i32, sheet: u64, name: impl Into<Arc<str>>) {
        self.z_buckets
            .entry(z)
            .or_default()
            .entry(sheet)
            .or_default()
            .push(name.into());
    }

    /// Match Purple's `operator[]` followed by `std::__find`/vector erase.
    /// Empty SpriteSheet and z nodes intentionally survive the removal.
    pub(crate) fn erase_first(&mut self, z: i32, sheet: u64, name: &str) {
        let names = self
            .z_buckets
            .entry(z)
            .or_default()
            .entry(sheet)
            .or_default();
        if let Some(index) = names.iter().position(|entry| entry.as_ref() == name) {
            names.remove(index);
        }
    }

    pub(crate) fn move_sheet(&mut self, z: i32, old_sheet: u64, new_sheet: u64, name: Arc<str>) {
        if old_sheet == new_sheet {
            return;
        }
        self.erase_first(z, old_sheet, name.as_ref());
        self.append(z, new_sheet, name);
    }

    pub(crate) fn move_z(&mut self, old_z: i32, new_z: i32, sheet: u64, name: Arc<str>) {
        // sub_1000592C4 performs both operations even when FCVTZS maps the
        // old and new values to the same integer bucket.
        self.erase_first(old_z, sheet, name.as_ref());
        self.append(new_z, sheet, name);
    }

    /// Destroy the complete GameLua `+0x310` tree. `loadLevel` uses the
    /// container destructor/reset path, unlike ordinary `removeObject`, whose
    /// empty inner and outer nodes remain observable until this boundary.
    pub(crate) fn clear(&mut self) {
        self.z_buckets.clear();
    }

    pub(crate) fn next_z_in_range(
        &self,
        minimum: i32,
        maximum: i32,
        after: Option<i32>,
    ) -> Option<i32> {
        if minimum >= maximum {
            return None;
        }
        let lower = match after {
            Some(current) if current >= minimum => Excluded(current),
            _ => Included(minimum),
        };
        self.z_buckets
            .range((lower, Excluded(maximum)))
            .next()
            .map(|(&z, _)| z)
    }

    pub(crate) fn first_sheet(&self, z: i32) -> Option<u64> {
        self.z_buckets
            .get(&z)?
            .first_key_value()
            .map(|(&sheet, _)| sheet)
    }

    pub(crate) fn next_sheet(&self, z: i32, after: u64) -> Option<u64> {
        self.z_buckets
            .get(&z)?
            .range((Excluded(after), Unbounded))
            .next()
            .map(|(&sheet, _)| sheet)
    }

    pub(crate) fn name_at(&self, z: i32, sheet: u64, index: usize) -> Option<Arc<str>> {
        self.z_buckets.get(&z)?.get(&sheet)?.get(index).cloned()
    }

    #[cfg(test)]
    pub(crate) fn entries_in_range(&self, minimum: i32, maximum: i32) -> Vec<(i32, String)> {
        if minimum >= maximum {
            return Vec::new();
        }
        self.z_buckets
            .range((Included(minimum), Excluded(maximum)))
            .flat_map(|(&z, sheets)| {
                sheets.values().flat_map(move |names| {
                    names.iter().map(move |name| (z, name.as_ref().to_owned()))
                })
            })
            .collect()
    }

    #[cfg(test)]
    pub(crate) fn contains_z(&self, z: i32) -> bool {
        self.z_buckets.contains_key(&z)
    }
}

pub(crate) fn native_scene_sheet_id(object: &SceneObject) -> u64 {
    object
        .sprite_region
        .as_ref()
        .map(|region| region.native_sheet_id)
        .or_else(|| object.composite_sprite.as_ref()?.first_native_sheet_id())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn moves_retain_the_supplied_cow_string_owner() {
        let mut index = NativeSceneRenderIndex::default();
        let name: Arc<str> = Arc::from("body");
        index.append(1, 10, Arc::clone(&name));

        index.move_z(1, 2, 10, Arc::clone(&name));
        let after_z = index.name_at(2, 10, 0).unwrap();
        assert!(Arc::ptr_eq(&after_z, &name));

        index.move_sheet(2, 10, 20, Arc::clone(&name));
        let after_sheet = index.name_at(2, 20, 0).unwrap();
        assert!(Arc::ptr_eq(&after_sheet, &name));
    }
}
