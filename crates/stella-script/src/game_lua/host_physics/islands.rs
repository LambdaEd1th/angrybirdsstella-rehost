//! Discrete `b2World::Solve` and `b2Island::Solve` orchestration.

use super::{
    MAX_ROTATION, MAX_TRANSLATION, PHYSICS_STEP, POSITION_ITERATIONS, VELOCITY_ITERATIONS,
};
use crate::*;

impl StellaLua {
    pub(super) fn solve_discrete_islands(
        &self,
        mut contact_events: Vec<ContactEvent>,
    ) -> (Vec<ContactEvent>, BTreeMap<String, NativeSweepStart>) {
        let mut bridge = self.render.lock().expect("render bridge lock poisoned");
        // SolveTOI needs the pre-island sweep only for bodies that can move.
        // Static endpoints retain an identical transform and are read from
        // the live world, matching Box2D's persistent per-body sweep without
        // cloning unrelated render payloads for every fixed step.
        let sweep_starts = bridge
            .scene
            .iter()
            .filter(|(_, object)| object.moves_during_step())
            .map(|(name, object)| (name.clone(), NativeSweepStart::capture(object)))
            .collect();
        bridge.assemble_box2d_islands();
        let gravity = (bridge.world_gravity_x, bridge.world_gravity_y);

        // sub_10086E634 invokes b2Island::Solve immediately after each DFS,
        // before advancing to the next world-list seed.
        // Purple's b2Island arrays are per-Step scratch storage. Move the
        // assembled arrays into this solve instead of deep-cloning every
        // body/contact/joint name before the first iteration.
        let mut islands = std::mem::take(&mut bridge.solver_islands);
        let mut synchronized_bodies = std::mem::take(&mut bridge.solver_synchronized_bodies);
        for island in &islands {
            bridge.integrate_island_velocities(&island.bodies, gravity, PHYSICS_STEP);
            trace_physics_body(&bridge, "after-force");
            // sub_10086CE84 warm-starts contacts before track and standard
            // joints; each pass then solves standard joints, tracks, contacts.
            bridge.seed_island_contact_velocity_constraints(&island.contacts);
            bridge.begin_island_contact_step(&island.contacts);
            bridge.begin_island_track_step(&island.bodies);
            bridge.begin_island_joint_step(&island.joints, PHYSICS_STEP);
            // b2Island constructs one compact body-indexed velocity array and
            // every contact iteration reuses it. Joint/track solvers in this
            // rehost still publish through the name-addressed scene, so only
            // refresh the three velocity scalars before each contact pass.
            bridge.begin_contact_velocity_cache(&island.contacts);
            let mut island_impulses = vec![0.0_f64; island.contacts.len()];
            for _ in 0..VELOCITY_ITERATIONS {
                bridge.solve_island_joints(&island.joints, PHYSICS_STEP, true, false);
                bridge.solve_island_track_velocity_constraints(&island.bodies);
                bridge.refresh_contact_velocity_cache();
                let pass_impulses = bridge
                    .solve_prepared_island_contact_velocity_constraint_values_once(
                        &island.contacts,
                    );
                bridge.commit_contact_velocity_cache();
                for (maximum, impulse) in island_impulses.iter_mut().zip(pass_impulses) {
                    *maximum = maximum.max(impulse);
                }
            }
            bridge.end_contact_velocity_cache();
            let island_impulses = island
                .contacts
                .iter()
                .cloned()
                .zip(island_impulses)
                .collect::<BTreeMap<_, _>>();
            for event in &mut contact_events {
                let key = (
                    event.first.clone(),
                    event.second.clone(),
                    event.first_fixture,
                    event.second_fixture,
                );
                if let Some(impulse) = island_impulses.get(&key) {
                    event.impulse = event.impulse.max(*impulse);
                }
            }
            bridge.store_island_contact_impulses(&island.contacts);

            bridge.integrate_island_positions(
                &island.bodies,
                PHYSICS_STEP,
                MAX_TRANSLATION,
                MAX_ROTATION,
            );
            let mut positions_solved = false;
            for _ in 0..POSITION_ITERATIONS {
                let contacts_solved = bridge.solve_island_contact_positions(&island.contacts);
                let joints_solved =
                    bridge.solve_island_joints(&island.joints, PHYSICS_STEP, false, true);
                positions_solved = contacts_solved && joints_solved;
                if positions_solved
                    && std::env::var_os("STELLA_FORCE_POSITION_ITERATIONS").is_none()
                {
                    break;
                }
            }
            bridge.update_single_box2d_island_sleep(island, PHYSICS_STEP, positions_solved);
        }
        contact_events.retain(|event| event.began || event.ended || event.impulse > f64::EPSILON);
        trace_physics_body(&bridge, "after-velocity");
        // sub_10086E634 walks the native world body list after all islands and
        // calls SynchronizeFixtures only when the body still carries its
        // island flag and its type is non-static. Assembly retained that
        // exact head-to-tail subset; do not reconstruct and sort it by name.
        bridge.sync_native_broad_phase_bodies(synchronized_bodies.iter().map(String::as_str));
        // Return the outer vectors as scratch capacity for the next fixed
        // step. The contained native island records are no longer live.
        islands.clear();
        synchronized_bodies.clear();
        bridge.solver_islands = islands;
        bridge.solver_synchronized_bodies = synchronized_bodies;
        (contact_events, sweep_starts)
    }
}
