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
        let two_pi = 2.0_f32 * std::f32::consts::PI;
        let turns = (self.angle_0 / two_pi).floor();
        self.angle_0 -= turns * two_pi;
        self.angle -= turns * two_pi;
    }

    pub(crate) fn transform(self, alpha: f32) -> NativeToiTransform {
        let angle = (self.angle - self.angle_0).mul_add(alpha, self.angle_0);
        let center = (
            (self.center.0 - self.center_0.0).mul_add(alpha, self.center_0.0),
            (self.center.1 - self.center_0.1).mul_add(alpha, self.center_0.1),
        );
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
        (
            local
                .0
                .mul_add(self.cosine, (-local.1).mul_add(self.sine, self.position.0)),
            local
                .0
                .mul_add(self.sine, local.1.mul_add(self.cosine, self.position.1)),
        )
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

#[derive(Debug, Clone)]
pub(crate) struct NativeToiContact {
    pub(crate) key: ContactKey,
    pub(crate) dynamic_body: String,
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
pub(crate) use separation::native_time_of_impact;
