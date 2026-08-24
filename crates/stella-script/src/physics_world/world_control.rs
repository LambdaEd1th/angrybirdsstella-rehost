//! GameLua's named physics-lock state machine.

use crate::*;

impl RenderBridge {
    pub(crate) fn set_physics_enabled(&mut self, enabled: bool, lock_name: String) {
        let count = self.physics_locks.entry(lock_name.clone()).or_default();
        if lock_name.is_empty() {
            // sub_100041ABC treats the unnamed lock as an idempotent switch.
            if enabled && *count != 0 {
                self.physics_lock_count = self.physics_lock_count.saturating_sub(*count);
                *count = 0;
            } else if !enabled && *count == 0 {
                *count = 1;
                self.physics_lock_count = self.physics_lock_count.saturating_add(1);
            }
        } else if enabled {
            if *count != 0 {
                *count -= 1;
                self.physics_lock_count = self.physics_lock_count.saturating_sub(1);
            }
        } else {
            *count = count.saturating_add(1);
            self.physics_lock_count = self.physics_lock_count.saturating_add(1);
        }
        self.physics_enabled = self.physics_lock_count == 0;
    }

    pub(crate) fn unlock_physics_lock(&mut self, lock_name: &str) {
        // sub_100042208 removes every outstanding reference for one named
        // lock, rather than behaving like a single setPhysicsEnabled(true).
        let count = self.physics_locks.entry(lock_name.to_owned()).or_default();
        self.physics_lock_count = self.physics_lock_count.saturating_sub(*count);
        *count = 0;
        self.physics_enabled = self.physics_lock_count == 0;
    }
}
