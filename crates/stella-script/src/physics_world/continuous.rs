//! GJK distance, separation functions and conservative TOI advancement.

use std::collections::BTreeMap;

use crate::{ContactKey, ContactManifold, SceneObject};

#[derive(Debug, Clone)]
pub(crate) struct NativeDistanceProxy {
    pub(crate) vertices: Vec<(f32, f32)>,
    pub(crate) radius: f32,
}

impl NativeDistanceProxy {
    pub(crate) fn support(&self, direction: (f32, f32)) -> usize {
        let mut best_index = 0;
        let mut best_value = self.vertices[0]
            .0
            .mul_add(direction.0, self.vertices[0].1 * direction.1);
        for (index, vertex) in self.vertices.iter().copied().enumerate().skip(1) {
            let value = vertex.0.mul_add(direction.0, vertex.1 * direction.1);
            if value > best_value {
                best_index = index;
                best_value = value;
            }
        }
        best_index
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct NativeSweep {
    pub(crate) local_center: (f32, f32),
    pub(crate) center_0: (f32, f32),
    pub(crate) center: (f32, f32),
    pub(crate) angle_0: f32,
    pub(crate) angle: f32,
}

/// The part of `b2Sweep` that must survive the discrete island solve so the
/// following TOI pass can reconstruct the body's motion.  Native Box2D keeps
/// these three float32 values on `b2Body`; retaining an entire `SceneObject`
/// here copied render resources and fixture vectors once per moving body and
/// fixed step.
#[derive(Debug, Clone, Copy)]
pub(crate) struct NativeSweepStart {
    pub(crate) center: (f32, f32),
    pub(crate) angle: f32,
}

impl NativeSweepStart {
    pub(crate) fn capture(object: &SceneObject) -> Self {
        Self {
            center: object.native_world_center(),
            angle: object.angle as f32,
        }
    }
}

impl NativeSweep {
    pub(crate) fn between(start: NativeSweepStart, end: &SceneObject) -> Self {
        let local_center = end.local_center();
        Self {
            local_center: (local_center.0 as f32, local_center.1 as f32),
            center_0: start.center,
            center: end.native_world_center(),
            angle_0: start.angle,
            angle: end.angle as f32,
        }
    }

    pub(crate) fn normalize(&mut self) {
        const NATIVE_INV_TWO_PI: f32 = f32::from_bits(0x3e22_f983);
        const NATIVE_TWO_PI: f32 = f32::from_bits(0x40c9_0fdb);
        let turns = (self.angle_0 * NATIVE_INV_TWO_PI).floor();
        let correction = turns * NATIVE_TWO_PI;
        self.angle_0 -= correction;
        self.angle -= correction;
    }

    pub(crate) fn transform(self, alpha: f32) -> NativeToiTransform {
        let (center, angle) = self.pose(alpha);
        let (sine, cosine) = angle.sin_cos();
        let rotated_center = (
            self.local_center
                .0
                .mul_add(cosine, -(self.local_center.1 * sine)),
            self.local_center
                .0
                .mul_add(sine, self.local_center.1 * cosine),
        );
        NativeToiTransform {
            position: (center.0 - rotated_center.0, center.1 - rotated_center.1),
            sine,
            cosine,
        }
    }

    pub(crate) fn pose(self, alpha: f32) -> ((f32, f32), f32) {
        // b2Sweep::GetTransform first rounds alpha * end, then fuses
        // (1 - alpha) * start into that value. This is observably different
        // from start + alpha * (end - start).
        let one_minus_alpha = 1.0_f32 - alpha;
        let center = (
            one_minus_alpha.mul_add(self.center_0.0, alpha * self.center.0),
            one_minus_alpha.mul_add(self.center_0.1, alpha * self.center.1),
        );
        let angle = one_minus_alpha.mul_add(self.angle_0, alpha * self.angle);
        (center, angle)
    }

    pub(crate) fn advance_pose(self, alpha: f32) -> NativeSweepStart {
        // b2Sweep::Advance uses the same weighted-endpoint sequence as
        // GetTransform before committing the result to c0/a0.
        let (center, angle) = self.pose(alpha);
        NativeSweepStart { center, angle }
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct NativeToiTransform {
    pub(crate) position: (f32, f32),
    pub(crate) sine: f32,
    pub(crate) cosine: f32,
}

impl NativeToiTransform {
    #[cfg(test)]
    pub(crate) const IDENTITY: Self = Self {
        position: (0.0, 0.0),
        sine: 0.0,
        cosine: 1.0,
    };

    pub(crate) fn point(self, local: (f32, f32)) -> (f32, f32) {
        // Every recovered b2Mul(transform, point) call completes the rotation
        // before adding transform.p with two independent FADDs. This is used
        // by both the discrete collision leaves and b2Distance/TOI.
        let rotated = self.rotate(local);
        (rotated.0 + self.position.0, rotated.1 + self.position.1)
    }

    pub(crate) fn rotate(self, local: (f32, f32)) -> (f32, f32) {
        (
            local.0.mul_add(self.cosine, -(local.1 * self.sine)),
            local.0.mul_add(self.sine, local.1 * self.cosine),
        )
    }

    pub(crate) fn inverse_rotate(self, world: (f32, f32)) -> (f32, f32) {
        (
            world.0.mul_add(self.cosine, world.1 * self.sine),
            (-world.0).mul_add(self.sine, world.1 * self.cosine),
        )
    }

    pub(crate) fn inverse_point(self, world: (f32, f32)) -> (f32, f32) {
        let relative = (world.0 - self.position.0, world.1 - self.position.1);
        self.inverse_rotate(relative)
    }
}

#[cfg(test)]
mod transform_tests {
    use super::{NativeSweep, NativeToiTransform};

    #[test]
    fn sweep_pose_weights_end_before_fusing_start() {
        let alpha = f32::from_bits(0x3e75_dd92);
        let start = f32::from_bits(0xc427_cc7d);
        let end = f32::from_bits(0xc348_4daa);
        let sweep = NativeSweep {
            local_center: (0.0, 0.0),
            center_0: (start, start),
            center: (end, end),
            angle_0: start,
            angle: end,
        };
        let (center, angle) = sweep.pose(alpha);
        assert_eq!(center.0.to_bits(), 0xc40b_887d);
        assert_eq!(center.1.to_bits(), 0xc40b_887d);
        assert_eq!(angle.to_bits(), 0xc40b_887d);

        let difference_form = (end - start).mul_add(alpha, start);
        assert_eq!(difference_form.to_bits(), 0xc40b_887c);
    }

    #[test]
    fn sweep_advance_commits_the_native_weighted_pose() {
        let alpha = f32::from_bits(0x3e75_dd92);
        let start = f32::from_bits(0xc427_cc7d);
        let end = f32::from_bits(0xc348_4daa);
        let advanced = NativeSweep {
            local_center: (0.0, 0.0),
            center_0: (start, start),
            center: (end, end),
            angle_0: start,
            angle: end,
        }
        .advance_pose(alpha);

        assert_eq!(advanced.center.0.to_bits(), 0xc40b_887d);
        assert_eq!(advanced.center.1.to_bits(), 0xc40b_887d);
        assert_eq!(advanced.angle.to_bits(), 0xc40b_887d);
    }

    #[test]
    fn sweep_normalize_multiplies_by_native_inverse_two_pi() {
        let angle = f32::from_bits(0x4476_9d72);
        let mut sweep = NativeSweep {
            local_center: (0.0, 0.0),
            center_0: (0.0, 0.0),
            center: (0.0, 0.0),
            angle_0: angle,
            angle: angle + 1.0,
        };
        sweep.normalize();
        assert_eq!(sweep.angle_0.to_bits(), 0x40c9_0f80);
        assert_eq!(
            sweep.angle.to_bits(),
            (f32::from_bits(0x40c9_0f80) + 1.0).to_bits()
        );

        let division_turns = (angle / f32::from_bits(0x40c9_0fdb)).floor();
        let division_form = angle - division_turns * f32::from_bits(0x40c9_0fdb);
        assert_eq!(division_form.to_bits(), 0xb880_0000);
    }

    #[test]
    fn point_adds_translation_after_native_rotation() {
        let transform = NativeToiTransform {
            position: (f32::from_bits(0x4028_c2d0), f32::from_bits(0xc06f_b7ef)),
            sine: f32::from_bits(0x3f54_5d1c),
            cosine: f32::from_bits(0x3f0e_f5d8),
        };
        let local = (f32::from_bits(0xc089_0dc8), f32::from_bits(0xc31f_1ccf));
        let native = transform.point(local);
        assert_eq!(native.0.to_bits(), 0x4304_3c7b);
        assert_eq!(native.1.to_bits(), 0xc2c0_4e62);

        let fused_translation = (
            local.0.mul_add(
                transform.cosine,
                (-local.1).mul_add(transform.sine, transform.position.0),
            ),
            local.0.mul_add(
                transform.sine,
                local.1.mul_add(transform.cosine, transform.position.1),
            ),
        );
        assert_eq!(fused_translation.0.to_bits(), 0x4304_3c7c);
        assert_eq!(fused_translation.1.to_bits(), 0xc2c0_4e63);
    }
}

#[derive(Debug, Clone)]
pub(crate) struct NativeToiContact {
    pub(crate) key: ContactKey,
    pub(crate) toi_bodies: (String, String),
    pub(crate) alpha: f32,
    pub(crate) manifold: ContactManifold,
}

#[derive(Debug, Default)]
pub(crate) struct NativeToiStepState {
    pub(crate) cached_world_alphas: BTreeMap<ContactKey, f32>,
    pub(crate) counts: BTreeMap<ContactKey, u8>,
}

impl NativeToiStepState {
    pub(crate) fn invalidate_body(&mut self, body: &str) {
        self.cached_world_alphas
            .retain(|(first, second, _, _), _| first != body && second != body);
    }
}

fn native_toi_dot(first: (f32, f32), second: (f32, f32)) -> f32 {
    first.0.mul_add(second.0, first.1 * second.1)
}

fn native_toi_sub(first: (f32, f32), second: (f32, f32)) -> (f32, f32) {
    (first.0 - second.0, first.1 - second.1)
}

fn native_toi_cross(first: (f32, f32), second: (f32, f32)) -> f32 {
    first.0.mul_add(second.1, -(first.1 * second.0))
}

mod distance;
mod separation;
mod simplex;

pub(crate) use distance::native_core_distance;
pub(crate) use separation::{NativeToiOutput, NativeToiState, native_time_of_impact};
pub(crate) use simplex::NativeSimplexCache;
