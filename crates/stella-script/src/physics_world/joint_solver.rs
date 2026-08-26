//! Island-level Box2D joint constraint dispatch.

mod body;
mod impulses;

pub(crate) use body::{JointBodyState, JointBodyView};

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
        self.solve_island_joints(&joint_names, step, solve_velocity, solve_position)
    }

    pub(crate) fn solve_island_joints(
        &mut self,
        joint_names: &[String],
        step: f64,
        solve_velocity: bool,
        solve_position: bool,
    ) -> bool {
        // b2Island retains stable b2Joint pointers and compact body position /
        // velocity arrays. Detach the persistent joint map while this pass
        // mutates scene bodies so neither the full joint nor either complete
        // render object has to be cloned for every solver iteration.
        let mut joints = std::mem::take(&mut self.joints);
        let mut broken = Vec::new();
        let mut positions_solved = true;
        for name in joint_names {
            let Some(joint) = joints.get_mut(name) else {
                continue;
            };
            let Some(first) = self.scene.get(&joint.first).map(JointBodyState::capture) else {
                broken.push(joint.name.clone());
                continue;
            };
            let Some(second) = self.scene.get(&joint.second).map(JointBodyState::capture) else {
                broken.push(joint.name.clone());
                continue;
            };
            let first_moving = first.participates_in_solve();
            let second_moving = second.participates_in_solve();
            if solve_velocity && joint.is_physical && (first_moving || second_moving) {
                match joint.joint_type {
                    1 => self.solve_distance_joint_velocity(joint, &first, &second, step),
                    2 => self.solve_weld_joint_velocity(joint, &first, &second),
                    3 => self.solve_revolute_joint_velocity(joint, &first, &second, step),
                    4 | 5 => self.solve_prismatic_joint_velocity(joint, &first, &second, step),
                    6 => self.solve_rope_joint_velocity(joint, &first, &second, step),
                    _ => {}
                }
            }
            if !solve_position || !joint.is_physical || (!first_moving && !second_moving) {
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
        self.joints = joints;
        for name in broken {
            self.destroy_native_joint(&name);
        }
        positions_solved
    }

    #[cfg(test)]
    pub(crate) fn begin_joint_step(&mut self, step: f64) {
        let joint_names = self.joints.keys().cloned().collect::<Vec<_>>();
        self.begin_island_joint_step(&joint_names, step);
    }

    pub(crate) fn begin_island_joint_step(&mut self, joint_names: &[String], step: f64) {
        if std::env::var_os("STELLA_DISABLE_JOINT_WARM_START").is_some() {
            self.clear_joint_impulses(joint_names, step);
            return;
        }
        let mut joints = std::mem::take(&mut self.joints);
        for name in joint_names {
            let Some(joint) = joints.get_mut(name) else {
                continue;
            };
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
        self.joints = joints;
    }
}
