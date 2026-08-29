//! Scene-body wrapper around `b2TestOverlap` (`sub_10086021C`).

use crate::*;

pub(crate) fn native_distance_proxies_overlap(
    first_proxy: &NativeDistanceProxy,
    first_transform: NativeToiTransform,
    second_proxy: &NativeDistanceProxy,
    second_transform: NativeToiTransform,
) -> bool {
    let mut cache = NativeSimplexCache::default();
    let core_distance = native_core_distance(
        first_proxy,
        first_transform,
        second_proxy,
        second_transform,
        &mut cache,
    );
    let combined_radius = first_proxy.radius + second_proxy.radius;
    let distance = if core_distance > combined_radius && core_distance > f32::EPSILON {
        core_distance - combined_radius
    } else {
        0.0_f32
    };
    distance < f32::from_bits(0x35a0_0000) // 10 * FLT_EPSILON
}

impl SceneObject {
    /// Run the native radius-aware GJK overlap test for one concrete fixture
    /// pair. `b2Contact::Update` uses this path for sensors instead of calling
    /// the shape-pair collision routine that constructs a solid manifold.
    pub(crate) fn native_fixture_overlaps(
        &self,
        other: &Self,
        first_fixture: usize,
        second_fixture: usize,
    ) -> bool {
        let Some(first_proxy) = self.native_distance_proxy(first_fixture) else {
            return false;
        };
        let Some(second_proxy) = other.native_distance_proxy(second_fixture) else {
            return false;
        };
        native_distance_proxies_overlap(
            &first_proxy,
            self.native_collision_transform(),
            &second_proxy,
            other.native_collision_transform(),
        )
    }

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
        self.native_fixture_overlaps(other, first_fixture, second_fixture)
    }
}
