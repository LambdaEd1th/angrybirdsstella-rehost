//! Copy native local manifold witnesses into position-constraint state.

use super::model::{PositionContactConstraint, PositionContactManifold};
use crate::*;

impl PositionContactConstraint {
    pub(crate) fn from_manifold(
        first: &SceneObject,
        second: &SceneObject,
        manifold: ContactManifold,
    ) -> Self {
        let first_local_center = first.local_center();
        let second_local_center = second.local_center();
        let local = manifold.native_local_position();
        let points = || {
            local.local_points[..usize::from(local.point_count)]
                .iter()
                .map(|&(x, y)| (f64::from(x), f64::from(y)))
                .collect::<Vec<_>>()
        };
        let position_manifold = match local.manifold_type {
            ContactManifoldType::Circles => PositionContactManifold::Circles {
                local_first: (
                    f64::from(local.local_point.0),
                    f64::from(local.local_point.1),
                ),
                local_second: (
                    f64::from(local.local_points[0].0),
                    f64::from(local.local_points[0].1),
                ),
                first_radius: f64::from(local.first_radius),
                second_radius: f64::from(local.second_radius),
            },
            ContactManifoldType::FaceFirst => PositionContactManifold::FaceFirst {
                local_normal: (
                    f64::from(local.local_normal.0),
                    f64::from(local.local_normal.1),
                ),
                local_plane_point: (
                    f64::from(local.local_point.0),
                    f64::from(local.local_point.1),
                ),
                local_clip_points: points(),
                first_radius: f64::from(local.first_radius),
                second_radius: f64::from(local.second_radius),
            },
            ContactManifoldType::FaceSecond => PositionContactManifold::FaceSecond {
                local_normal: (
                    f64::from(local.local_normal.0),
                    f64::from(local.local_normal.1),
                ),
                local_plane_point: (
                    f64::from(local.local_point.0),
                    f64::from(local.local_point.1),
                ),
                local_clip_points: points(),
                first_radius: f64::from(local.first_radius),
                second_radius: f64::from(local.second_radius),
            },
        };
        Self {
            first_local_center: (first_local_center.0 as f32, first_local_center.1 as f32),
            second_local_center: (second_local_center.0 as f32, second_local_center.1 as f32),
            first_inverse_mass: first.inverse_mass_for_solver() as f32,
            second_inverse_mass: second.inverse_mass_for_solver() as f32,
            first_inverse_inertia: first.inverse_inertia() as f32,
            second_inverse_inertia: second.inverse_inertia() as f32,
            manifold: position_manifold,
        }
    }

    pub(crate) fn point_count(&self) -> usize {
        match &self.manifold {
            PositionContactManifold::Circles { .. } => 1,
            PositionContactManifold::FaceFirst {
                local_clip_points, ..
            }
            | PositionContactManifold::FaceSecond {
                local_clip_points, ..
            } => local_clip_points.len(),
        }
    }
}
