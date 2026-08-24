//! Convert contact manifold witnesses into local position-constraint state.

use super::model::PositionContactConstraint;
use crate::*;

impl PositionContactConstraint {
    pub(crate) fn from_manifold(
        first: &SceneObject,
        second: &SceneObject,
        manifold: ContactManifold,
    ) -> Self {
        let world_normal = (manifold.normal_x, manifold.normal_y);
        let first_circle = first.collision_circle();
        let second_circle = second.collision_circle();
        if let (Some((first_center, first_radius)), Some((second_center, second_radius))) =
            (first_circle, second_circle)
        {
            return Self::Circles {
                local_first: first.native_inverse_transform_body_point(first_center),
                local_second: second.native_inverse_transform_body_point(second_center),
                first_radius,
                second_radius,
            };
        }

        let first_radius = first_circle
            .map(|(_, radius)| radius)
            .unwrap_or(BOX2D_POLYGON_RADIUS);
        let second_radius = second_circle
            .map(|(_, radius)| radius)
            .unwrap_or(BOX2D_POLYGON_RADIUS);
        let points = manifold.points();
        let face_is_first = matches!(manifold.manifold_type, ContactManifoldType::FaceFirst);

        if face_is_first {
            let clip_points = if let Some((circle_center, _)) = second_circle {
                vec![circle_center]
            } else {
                points
                    .iter()
                    .map(|point| {
                        let geometric_separation = first_radius + second_radius - point.penetration;
                        (
                            point.point_x + world_normal.0 * geometric_separation * 0.5,
                            point.point_y + world_normal.1 * geometric_separation * 0.5,
                        )
                    })
                    .collect::<Vec<_>>()
            };
            let primary_separation = first_radius + second_radius - points[0].penetration;
            let plane_point = (
                clip_points[0].0 - world_normal.0 * primary_separation,
                clip_points[0].1 - world_normal.1 * primary_separation,
            );
            Self::FaceFirst {
                local_normal: inverse_rotate_vector(world_normal, first.angle),
                local_plane_point: first.native_inverse_transform_body_point(plane_point),
                local_clip_points: clip_points
                    .into_iter()
                    .map(|point| second.native_inverse_transform_body_point(point))
                    .collect(),
                first_radius,
                second_radius,
            }
        } else {
            let reference_normal = (-world_normal.0, -world_normal.1);
            let clip_points = if let Some((circle_center, _)) = first_circle {
                vec![circle_center]
            } else {
                points
                    .iter()
                    .map(|point| {
                        let geometric_separation = first_radius + second_radius - point.penetration;
                        (
                            point.point_x - world_normal.0 * geometric_separation * 0.5,
                            point.point_y - world_normal.1 * geometric_separation * 0.5,
                        )
                    })
                    .collect::<Vec<_>>()
            };
            let primary_separation = first_radius + second_radius - points[0].penetration;
            let plane_point = (
                clip_points[0].0 - reference_normal.0 * primary_separation,
                clip_points[0].1 - reference_normal.1 * primary_separation,
            );
            Self::FaceSecond {
                local_normal: inverse_rotate_vector(reference_normal, second.angle),
                local_plane_point: second.native_inverse_transform_body_point(plane_point),
                local_clip_points: clip_points
                    .into_iter()
                    .map(|point| first.native_inverse_transform_body_point(point))
                    .collect(),
                first_radius,
                second_radius,
            }
        }
    }

    pub(crate) fn point_count(&self) -> usize {
        match self {
            Self::Circles { .. } => 1,
            Self::FaceFirst {
                local_clip_points, ..
            }
            | Self::FaceSecond {
                local_clip_points, ..
            } => local_clip_points.len(),
        }
    }
}
