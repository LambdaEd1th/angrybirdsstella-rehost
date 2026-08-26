//! Per-island ownership of the stable joint-pointer array used by Box2D.

use crate::*;

#[derive(Debug)]
pub(crate) struct NativeIslandJointConstraint {
    pub(super) map_key: String,
    pub(super) joint: PhysicsJoint,
    pub(super) broken: bool,
}

/// Persistent joint records selected for one island. Purple keeps the same
/// `b2Joint*` array live from InitVelocityConstraints through every velocity
/// and position iteration; this owner gives the Rust solver the same lifetime
/// without repeated name-tree lookup or record cloning.
#[derive(Debug, Default)]
pub(crate) struct NativeIslandJointConstraints {
    pub(super) entries: Vec<NativeIslandJointConstraint>,
}

impl NativeIslandJointConstraints {
    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        self.entries.len()
    }
}

impl RenderBridge {
    pub(crate) fn take_island_joint_constraints(
        &mut self,
        joint_names: &[String],
    ) -> NativeIslandJointConstraints {
        let mut constraints = std::mem::take(&mut self.joint_constraint_scratch);
        debug_assert!(constraints.entries.is_empty());
        for name in joint_names {
            let Some((map_key, joint)) = self.joints.remove_entry(name) else {
                continue;
            };
            constraints.entries.push(NativeIslandJointConstraint {
                map_key,
                joint,
                broken: false,
            });
        }
        constraints
    }

    pub(crate) fn restore_island_joint_constraints(
        &mut self,
        mut constraints: NativeIslandJointConstraints,
    ) {
        for entry in constraints.entries.drain(..) {
            self.joints.insert(entry.map_key, entry.joint);
        }
        self.joint_constraint_scratch = constraints;
    }

    pub(super) fn retire_broken_island_joint_constraints(
        &mut self,
        constraints: &mut NativeIslandJointConstraints,
    ) {
        let mut index = 0;
        while index < constraints.entries.len() {
            if !constraints.entries[index].broken {
                index += 1;
                continue;
            }
            // Missing endpoints are not a valid native state, but retain the
            // previous adapter's recovery boundary: put the persistent record
            // back first, then run the ordinary ordered joint destructor.
            let entry = constraints.entries.remove(index);
            let name = entry.map_key.clone();
            self.joints.insert(entry.map_key, entry.joint);
            self.destroy_native_joint(&name);
        }
    }
}
