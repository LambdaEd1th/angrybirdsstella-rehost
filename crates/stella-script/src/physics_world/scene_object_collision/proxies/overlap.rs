//! Scene-body wrapper around `b2TestOverlap` (`sub_10086021C`).

use crate::*;

impl SceneObject {
    /// `sub_100032970` passes only each body's intrusive fixture-list head to
    /// `b2TestOverlap`. Fixtures are head-inserted natively, while this model
    /// retains creation order, so the selected fixture is the final entry.
    pub(crate) fn native_head_fixture_overlaps(&self, other: &Self) -> bool {
        let Some(first_fixture) = self.collision_shape.fixture_count().checked_sub(1) else {
            return false;
        };
        let Some(second_fixture) = other.collision_shape.fixture_count().checked_sub(1) else {
            return false;
        };
        let Some(first_proxy) = self.native_distance_proxy(first_fixture) else {
            return false;
        };
        let Some(second_proxy) = other.native_distance_proxy(second_fixture) else {
            return false;
        };
        let first_angle = self.angle as f32;
        let second_angle = other.angle as f32;
        let (first_sine, first_cosine) = first_angle.sin_cos();
        let (second_sine, second_cosine) = second_angle.sin_cos();
        let first_transform = NativeToiTransform {
            position: (self.x as f32, self.y as f32),
            sine: first_sine,
            cosine: first_cosine,
        };
        let second_transform = NativeToiTransform {
            position: (other.x as f32, other.y as f32),
            sine: second_sine,
            cosine: second_cosine,
        };
        let core_distance = native_core_distance(
            &first_proxy,
            first_transform,
            &second_proxy,
            second_transform,
        )
        .0;
        let combined_radius = first_proxy.radius + second_proxy.radius;
        let distance = if core_distance > combined_radius && core_distance > f32::EPSILON {
            core_distance - combined_radius
        } else {
            0.0_f32
        };
        distance < f32::from_bits(0x35a0_0000) // 10 * FLT_EPSILON
    }
}
