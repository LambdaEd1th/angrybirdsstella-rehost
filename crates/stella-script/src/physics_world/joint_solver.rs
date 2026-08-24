//! Island-level Box2D joint constraint dispatch.

mod impulses;

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
        let joints = joint_names
            .iter()
            .filter_map(|name| self.joints.get(name).cloned())
            .collect::<Vec<_>>();
        let mut broken = Vec::new();
        let mut positions_solved = true;
        for joint in joints {
            let Some(first) = self.scene.get(&joint.first).cloned() else {
                broken.push(joint.name);
                continue;
            };
            let Some(second) = self.scene.get(&joint.second).cloned() else {
                broken.push(joint.name);
                continue;
            };
            let first_moving = first.moves_during_step()
                && first.active
                && first.motion_started
                && !first.sleeping;
            let second_moving = second.moves_during_step()
                && second.active
                && second.motion_started
                && !second.sleeping;
            if solve_velocity && joint.is_physical && (first_moving || second_moving) {
                match joint.joint_type {
                    1 => self.solve_distance_joint_velocity(&joint, &first, &second, step),
                    2 => self.solve_weld_joint_velocity(&joint, &first, &second),
                    3 => self.solve_revolute_joint_velocity(&joint, &first, &second, step),
                    4 | 5 => self.solve_prismatic_joint_velocity(&joint, &first, &second, step),
                    6 => self.solve_rope_joint_velocity(&joint, &first, &second, step),
                    _ => {}
                }
            }
            if !solve_position || !joint.is_physical || (!first_moving && !second_moving) {
                continue;
            }
            positions_solved &= match joint.joint_type {
                1 => self.solve_distance_joint_position(&joint, &first, &second),
                2 => self.solve_weld_joint_position(&joint, &first, &second),
                3 => self.solve_revolute_joint_position(&joint, &first, &second),
                4 | 5 => self.solve_prismatic_joint_position(&joint, &first, &second),
                6 => self.solve_rope_joint_position(&joint, &first, &second),
                _ => true,
            };
        }
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
        let joints = joint_names
            .iter()
            .filter_map(|name| self.joints.get(name).cloned())
            .collect::<Vec<_>>();
        for stale_joint in joints {
            let Some(first) = self.scene.get(&stale_joint.first).cloned() else {
                continue;
            };
            let Some(second) = self.scene.get(&stale_joint.second).cloned() else {
                continue;
            };
            let first_moving = first.moves_during_step()
                && first.active
                && first.motion_started
                && !first.sleeping;
            let second_moving = second.moves_during_step()
                && second.active
                && second.motion_started
                && !second.sleeping;
            if !first_moving && !second_moving {
                continue;
            }

            match stale_joint.joint_type {
                1 => self.initialize_distance_velocity_constraints(
                    &stale_joint,
                    &first,
                    &second,
                    step,
                ),
                2 => self.initialize_weld_velocity_constraints(&stale_joint, &first, &second, step),
                3 => self.initialize_revolute_velocity_constraints(
                    &stale_joint,
                    &first,
                    &second,
                    step,
                ),
                4 | 5 => self.initialize_prismatic_velocity_constraints(
                    &stale_joint,
                    &first,
                    &second,
                    step,
                ),
                6 => self.initialize_rope_velocity_constraints(&stale_joint, &first, &second, step),
                _ => self.scale_joint_impulses(&stale_joint.name, step),
            }
        }
    }
}
