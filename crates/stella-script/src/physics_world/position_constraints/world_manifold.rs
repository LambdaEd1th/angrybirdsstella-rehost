//! `b2PositionSolverManifold::Initialize` (`sub_100864CFC`).

use super::model::{PositionBodyState, PositionContactConstraint, PositionWorldPoint};
#[cfg(test)]
use crate::SceneObject;

impl PositionContactConstraint {
    #[cfg(test)]
    pub(crate) fn world_point(
        &self,
        first: &SceneObject,
        second: &SceneObject,
        index: usize,
    ) -> Option<PositionWorldPoint> {
        self.world_point_from_states(
            PositionBodyState::capture(first),
            PositionBodyState::capture(second),
            index,
        )
    }

    pub(crate) fn world_point_from_states(
        &self,
        first: PositionBodyState,
        second: PositionBodyState,
        index: usize,
    ) -> Option<PositionWorldPoint> {
        match self {
            Self::Circles {
                local_first,
                local_second,
                first_radius,
                second_radius,
            } => {
                let point_a = first.transform_point((local_first.0 as f32, local_first.1 as f32));
                let point_b =
                    second.transform_point((local_second.0 as f32, local_second.1 as f32));
                let delta = (point_b.0 - point_a.0, point_b.1 - point_a.1);
                let distance = delta.0.hypot(delta.1);
                let normal = if distance >= f32::EPSILON {
                    (delta.0 / distance, delta.1 / distance)
                } else {
                    (delta.0, delta.1)
                };
                (index == 0).then_some(PositionWorldPoint {
                    normal,
                    point: (
                        (point_a.0 + point_b.0) * 0.5_f32,
                        (point_a.1 + point_b.1) * 0.5_f32,
                    ),
                    separation: delta.0.mul_add(normal.0, delta.1 * normal.1)
                        - *first_radius as f32
                        - *second_radius as f32,
                })
            }
            Self::FaceFirst {
                local_normal,
                local_plane_point,
                local_clip_points,
                first_radius,
                second_radius,
            } => {
                let (sine, cosine) = first.angle.sin_cos();
                let local_normal = (local_normal.0 as f32, local_normal.1 as f32);
                let normal = (
                    local_normal.0.mul_add(cosine, -(local_normal.1 * sine)),
                    local_normal.0.mul_add(sine, local_normal.1 * cosine),
                );
                let plane_point =
                    first.transform_point((local_plane_point.0 as f32, local_plane_point.1 as f32));
                let local_clip_point = local_clip_points.get(index)?;
                let point =
                    second.transform_point((local_clip_point.0 as f32, local_clip_point.1 as f32));
                Some(PositionWorldPoint {
                    normal,
                    point,
                    separation: (point.0 - plane_point.0)
                        .mul_add(normal.0, (point.1 - plane_point.1) * normal.1)
                        - *first_radius as f32
                        - *second_radius as f32,
                })
            }
            Self::FaceSecond {
                local_normal,
                local_plane_point,
                local_clip_points,
                first_radius,
                second_radius,
            } => {
                let (sine, cosine) = second.angle.sin_cos();
                let local_normal = (local_normal.0 as f32, local_normal.1 as f32);
                let reference_normal = (
                    local_normal.0.mul_add(cosine, -(local_normal.1 * sine)),
                    local_normal.0.mul_add(sine, local_normal.1 * cosine),
                );
                let normal = (-reference_normal.0, -reference_normal.1);
                let plane_point = second
                    .transform_point((local_plane_point.0 as f32, local_plane_point.1 as f32));
                let local_clip_point = local_clip_points.get(index)?;
                let point =
                    first.transform_point((local_clip_point.0 as f32, local_clip_point.1 as f32));
                Some(PositionWorldPoint {
                    normal,
                    point,
                    separation: (point.0 - plane_point.0).mul_add(
                        reference_normal.0,
                        (point.1 - plane_point.1) * reference_normal.1,
                    ) - *first_radius as f32
                        - *second_radius as f32,
                })
            }
        }
    }
}
