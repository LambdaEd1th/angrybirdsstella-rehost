//! Island-level Box2D joint constraint dispatch.

mod body;
mod cache;
mod impulses;

pub(crate) use body::{JointBodyState, JointBodyView};
pub(crate) use cache::NativeIslandJointConstraints;

use crate::*;

impl RenderBridge {
    #[cfg(test)]
    pub(crate) fn solve_joints(
        &mut self,
        step: f64,
        solve_velocity: bool,
        solve_position: bool,
    ) -> bool {
        let joint_names = self.joints.keys().cloned().collect::<Vec<_>>();
        let mut constraints = self.take_island_joint_constraints(&joint_names);
        let solved = self.solve_island_joint_constraints(
            &mut constraints,
            step,
            solve_velocity,
            solve_position,
        );
        self.restore_island_joint_constraints(constraints);
        solved
    }

    pub(crate) fn solve_island_joint_constraints(
        &mut self,
        constraints: &mut NativeIslandJointConstraints,
        step: f64,
        solve_velocity: bool,
        solve_position: bool,
    ) -> bool {
        let mut positions_solved = true;
        for entry in &mut constraints.entries {
            let joint = &mut entry.joint;
            let Some(first) = self.scene.get(&joint.first).map(JointBodyState::capture) else {
                entry.broken = true;
                continue;
            };
            let Some(second) = self.scene.get(&joint.second).map(JointBodyState::capture) else {
                entry.broken = true;
                continue;
            };
            let first_moving = first.participates_in_solve();
            let second_moving = second.participates_in_solve();
            if solve_velocity && joint.has_native_joint() && (first_moving || second_moving) {
                match joint.joint_type {
                    1 => self.solve_distance_joint_velocity(joint, &first, &second, step),
                    2 => self.solve_weld_joint_velocity(joint, &first, &second),
                    3 => self.solve_revolute_joint_velocity(joint, &first, &second, step),
                    4 | 5 => self.solve_prismatic_joint_velocity(joint, &first, &second, step),
                    6 => self.solve_rope_joint_velocity(joint, &first, &second, step),
                    _ => {}
                }
            }
            if !solve_position || !joint.has_native_joint() || (!first_moving && !second_moving) {
                continue;
            }
            positions_solved &= match joint.joint_type {
                1 => self.solve_distance_joint_position(joint, &first, &second),
                2 => self.solve_weld_joint_position(joint, &first, &second),
                3 => self.solve_revolute_joint_position(joint, &first, &second),
                4 | 5 => self.solve_prismatic_joint_position(joint, &first, &second),
                6 => self.solve_rope_joint_position(joint, &first, &second),
                _ => true,
            };
        }
        self.retire_broken_island_joint_constraints(constraints);
        positions_solved
    }

    #[cfg(test)]
    pub(crate) fn begin_joint_step(&mut self, step: f64) {
        let joint_names = self.joints.keys().cloned().collect::<Vec<_>>();
        let mut constraints = self.take_island_joint_constraints(&joint_names);
        self.begin_island_joint_constraints(&mut constraints, step);
        self.restore_island_joint_constraints(constraints);
    }

    pub(crate) fn begin_island_joint_constraints(
        &mut self,
        constraints: &mut NativeIslandJointConstraints,
        step: f64,
    ) {
        if std::env::var_os("STELLA_DISABLE_JOINT_WARM_START").is_some() {
            Self::clear_constraint_impulses(constraints, step);
        }
        for entry in &mut constraints.entries {
            let joint = &mut entry.joint;
            if !joint.has_native_joint() {
                continue;
            }
            let Some(first) = self.scene.get(&joint.first).map(JointBodyState::capture) else {
                continue;
            };
            let Some(second) = self.scene.get(&joint.second).map(JointBodyState::capture) else {
                continue;
            };
            let first_moving = first.participates_in_solve();
            let second_moving = second.participates_in_solve();
            if !first_moving && !second_moving {
                continue;
            }

            match joint.joint_type {
                1 => self.initialize_distance_velocity_constraints(joint, &first, &second, step),
                2 => self.initialize_weld_velocity_constraints(joint, &first, &second, step),
                3 => self.initialize_revolute_velocity_constraints(joint, &first, &second, step),
                4 | 5 => {
                    self.initialize_prismatic_velocity_constraints(joint, &first, &second, step)
                }
                6 => self.initialize_rope_velocity_constraints(joint, &first, &second, step),
                _ => Self::scale_joint_impulses(joint, step),
            }
        }
    }
}
