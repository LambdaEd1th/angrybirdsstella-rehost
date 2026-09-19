//! Native contact callbacks, collision damage and joint-break propagation.

mod damage;
mod dispatch;
mod prepare;
mod score;

use crate::ContactPoint;

pub(crate) use damage::*;
pub(crate) use dispatch::*;
pub(crate) use prepare::*;
pub(crate) use score::*;

/// One contact per fixture pair, with bodies stored in native ContactFactory
/// fixture-A/fixture-B order.
pub(crate) type ContactKey = (String, String, usize, usize);

#[derive(Debug, Clone)]
pub(crate) struct ContactEvent {
    pub(crate) first: String,
    pub(crate) second: String,
    pub(crate) first_fixture: usize,
    pub(crate) second_fixture: usize,
    pub(crate) sensor: bool,
    pub(crate) began: bool,
    pub(crate) ended: bool,
    pub(crate) impulse: f64,
    pub(crate) normal_x: f64,
    pub(crate) normal_y: f64,
    pub(crate) point_x: f64,
    pub(crate) point_y: f64,
    pub(crate) first_mass: f64,
    pub(crate) first_velocity_x: f64,
    pub(crate) first_velocity_y: f64,
    pub(crate) second_mass: f64,
    pub(crate) second_velocity_x: f64,
    pub(crate) second_velocity_y: f64,
}

#[derive(Debug, Clone)]
pub(crate) enum NativeContactCallback {
    Enter {
        first: String,
        second: String,
    },
    Exit {
        first: String,
        second: String,
        sensor: bool,
    },
    Bird {
        first: String,
        second: String,
        force: f64,
        damage: f64,
        point_x: f64,
        point_y: f64,
        normal_x: f64,
        normal_y: f64,
    },
    Block {
        first: String,
        second: String,
        force: f64,
        damaged: bool,
        collision_damage: f64,
        point_x: f64,
        point_y: f64,
        normal_x: f64,
        normal_y: f64,
        score_damage: f64,
        previous_score: f32,
    },
}

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct NativeDamageResult {
    attempted: bool,
    reported_damage: f64,
    applied_damage: f64,
    /// Unrounded surviving damage (or previous strength for a dead block).
    /// None means the block-score branch was skipped by ignoreAllDamage or
    /// defence, so the corresponding ignoresScore byte is not consulted.
    score_damage: Option<f32>,
    previous_strength: f64,
    remaining_strength: f64,
    destroyed: bool,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct CollisionForceFactors {
    pub(crate) force_damage_multiplier: f64,
    pub(crate) damage_multiplier: f64,
    pub(crate) powerup_damage_multiplier: f64,
    pub(crate) velocity_multiplier: f64,
}

impl Default for CollisionForceFactors {
    fn default() -> Self {
        Self {
            force_damage_multiplier: 1.0,
            damage_multiplier: 1.0,
            powerup_damage_multiplier: 1.0,
            velocity_multiplier: 1.0,
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct CachedContactImpulse {
    pub(crate) normal: f64,
    pub(crate) tangent: f64,
    pub(crate) secondary_normal: f64,
    pub(crate) secondary_tangent: f64,
    pub(crate) primary_feature_id: u32,
    pub(crate) secondary_feature_id: u32,
    pub(crate) point_count: u8,
}

impl CachedContactImpulse {
    pub(crate) fn point(self, index: usize) -> (f64, f64) {
        if index == 0 {
            (self.normal, self.tangent)
        } else {
            (self.secondary_normal, self.secondary_tangent)
        }
    }

    pub(crate) fn set_point(&mut self, index: usize, normal: f64, tangent: f64) {
        if index == 0 {
            self.normal = normal;
            self.tangent = tangent;
        } else {
            self.secondary_normal = normal;
            self.secondary_tangent = tangent;
        }
    }

    pub(crate) fn aligned_to(self, points: &[ContactPoint]) -> Self {
        let old = [
            (self.primary_feature_id, self.normal, self.tangent),
            (
                self.secondary_feature_id,
                self.secondary_normal,
                self.secondary_tangent,
            ),
        ];
        let mut aligned = Self {
            point_count: points.len().min(2) as u8,
            ..Self::default()
        };
        for (new_index, point) in points.iter().take(2).enumerate() {
            if new_index == 0 {
                aligned.primary_feature_id = point.feature_id;
            } else {
                aligned.secondary_feature_id = point.feature_id;
            }
            if let Some((_, normal, tangent)) = old
                .iter()
                .take(usize::from(self.point_count))
                .find(|(feature_id, _, _)| *feature_id == point.feature_id)
            {
                aligned.set_point(new_index, *normal, *tangent);
            }
        }
        aligned
    }
}

pub(crate) fn contact_feature_id(index_a: usize, index_b: usize, type_a: u8, type_b: u8) -> u32 {
    (index_a.min(u8::MAX as usize) as u32)
        | ((index_b.min(u8::MAX as usize) as u32) << 8)
        | (u32::from(type_a) << 16)
        | (u32::from(type_b) << 24)
}

pub(crate) fn swap_contact_features(feature_id: u32) -> u32 {
    contact_feature_id(
        ((feature_id >> 8) & 0xff) as usize,
        (feature_id & 0xff) as usize,
        ((feature_id >> 24) & 0xff) as u8,
        ((feature_id >> 16) & 0xff) as u8,
    )
}
