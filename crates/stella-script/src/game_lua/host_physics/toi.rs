//! `b2World::SolveTOI` callback and reduced-island orchestration.

use super::{MAX_ROTATION, MAX_TRANSLATION, PHYSICS_STEP, VELOCITY_ITERATIONS};
use crate::*;

impl StellaLua {
    pub(super) fn solve_continuous_islands(
        &self,
        contact_events: &mut Vec<ContactEvent>,
        toi_sweep_starts: &mut BTreeMap<String, NativeSweepStart>,
    ) -> Result<(), ScriptError> {
        // SolveTOI invokes Contact::Update after advancing the chosen sweeps
        // but before solving the reduced island. Keep callbacks outside the
        // bridge lock and resume the contact-list scan after each mutation.
        let mut toi_sweep_alphas = BTreeMap::<String, f32>::new();
        let mut toi_state = NativeToiStepState::default();
        loop {
            let pending = {
                let mut bridge = self.render.lock().expect("render bridge lock poisoned");
                bridge.advance_continuous_tunneling(
                    toi_sweep_starts,
                    &toi_sweep_alphas,
                    &mut toi_state,
                )
            };
            let Some(mut pending) = pending else {
                break;
            };
            let selected = pending[0].0.clone();
            let mut callback_index = 0;
            while callback_index < pending.len() {
                let event = pending[callback_index].1.clone();
                let (contact_callbacks, broken_joints) = {
                    let mut bridge = self.render.lock().expect("render bridge lock poisoned");
                    prepare_native_contact_callbacks(
                        &self.lua,
                        &mut bridge,
                        std::slice::from_ref(&event),
                    )?
                };
                for name in &broken_joints {
                    dispatch_and_remove_lua_joint(&self.lua, name)?;
                }
                dispatch_native_contact_callbacks(&self.lua, &self.render, contact_callbacks)?;
                let island_contacts = pending
                    .iter()
                    .map(|(contact, _)| contact.key.clone())
                    .collect::<Vec<_>>();
                let auxiliary = self
                    .render
                    .lock()
                    .expect("render bridge lock poisoned")
                    .advance_next_toi_auxiliary_contact(
                        (&selected.toi_bodies.0, &selected.toi_bodies.1),
                        selected.alpha,
                        &island_contacts,
                        toi_sweep_starts,
                        &toi_sweep_alphas,
                    );
                if let Some(auxiliary) = auxiliary {
                    pending.push(auxiliary);
                }
                callback_index += 1;
            }
            let contacts = pending
                .iter()
                .map(|(contact, _)| contact.clone())
                .collect::<Vec<_>>();
            let (impulses, next_sweep_starts, cache_invalidation_bodies) = {
                let mut bridge = self.render.lock().expect("render bridge lock poisoned");
                bridge.finish_continuous_tunneling(
                    &contacts,
                    PHYSICS_STEP,
                    VELOCITY_ITERATIONS,
                    MAX_TRANSLATION,
                    MAX_ROTATION,
                )
            };
            for (body, next_sweep_start) in &next_sweep_starts {
                toi_sweep_starts.insert(body.clone(), *next_sweep_start);
            }
            for (contact, mut event) in pending.drain(..) {
                if let Some(impulse) = impulses.get(&contact.key) {
                    event.impulse = event.impulse.max(*impulse);
                }
                contact_events.push(event);
            }
            for body in next_sweep_starts.keys() {
                toi_sweep_alphas.insert(body.clone(), selected.alpha);
            }
            for body in cache_invalidation_bodies {
                toi_state.invalidate_body(&body);
            }
        }
        Ok(())
    }
}
