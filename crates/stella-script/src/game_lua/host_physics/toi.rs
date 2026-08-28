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
                        &selected.dynamic_body,
                        selected.alpha,
                        &island_contacts,
                    );
                if let Some(auxiliary) = auxiliary {
                    pending.push(auxiliary);
                }
                callback_index += 1;
            }
            let alpha_0 = toi_sweep_alphas
                .get(&selected.dynamic_body)
                .copied()
                .unwrap_or(0.0_f32);
            let body_step = PHYSICS_STEP * f64::from(1.0_f32 - alpha_0);
            let contacts = pending
                .iter()
                .map(|(contact, _)| contact.clone())
                .collect::<Vec<_>>();
            let (impulses, next_sweep_start) = {
                let mut bridge = self.render.lock().expect("render bridge lock poisoned");
                let (impulses, sweep_starts) = bridge.finish_continuous_tunneling(
                    &contacts,
                    body_step,
                    VELOCITY_ITERATIONS,
                    MAX_TRANSLATION,
                    MAX_ROTATION,
                );
                (impulses, sweep_starts.get(&selected.dynamic_body).copied())
            };
            if let Some(next_sweep_start) = next_sweep_start {
                toi_sweep_starts.insert(selected.dynamic_body.clone(), next_sweep_start);
            }
            for (contact, mut event) in pending.drain(..) {
                if let Some(impulse) = impulses.get(&contact.key) {
                    event.impulse = event.impulse.max(*impulse);
                }
                contact_events.push(event);
            }
            toi_sweep_alphas.insert(
                selected.dynamic_body.clone(),
                (1.0_f32 - alpha_0).mul_add(selected.alpha, alpha_0),
            );
            toi_state.invalidate_body(&selected.dynamic_body);
        }
        Ok(())
    }
}
