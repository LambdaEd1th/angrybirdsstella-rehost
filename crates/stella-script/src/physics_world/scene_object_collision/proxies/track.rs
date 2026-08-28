//! `objectAndTrackOverlap` (`sub_10003D208`) distance wrapper.

use super::overlap::native_distance_proxies_overlap;
use crate::*;

impl SceneObject {
    pub(crate) fn head_fixture_overlaps_track_segment(
        &self,
        track_segment: ((f64, f64), (f64, f64)),
    ) -> bool {
        // sub_10003D45C..0x10003D494 selects only b2Body::m_fixtureList,
        // creates one child proxy for this chain edge and calls the shared
        // b2TestOverlap wrapper. Fixture vectors retain creation order, so
        // the intrusive-list head is the final fixture.
        let Some(head_fixture) = self.collision_shape.fixture_count().checked_sub(1) else {
            return false;
        };
        let Some(object_proxy) = self.native_distance_proxy(head_fixture) else {
            return false;
        };
        let track_proxy = NativeDistanceProxy {
            vertices: vec![
                (track_segment.0.0 as f32, track_segment.0.1 as f32),
                (track_segment.1.0 as f32, track_segment.1.1 as f32),
            ],
            radius: BOX2D_POLYGON_RADIUS as f32,
        };
        native_distance_proxies_overlap(
            &object_proxy,
            self.native_collision_transform(),
            &track_proxy,
            NativeToiTransform::IDENTITY,
        )
    }
}
