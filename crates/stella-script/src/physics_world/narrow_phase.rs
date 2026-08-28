//! Box2D narrow phase split along Purple's recovered collision members.

mod circle;
mod edge;
mod geometry;
mod polygon;

use smallvec::SmallVec;

/// Purple embeds the Box2D 2.2 polygon vertex and normal arrays in each
/// `b2PolygonShape`. Authored contours are decomposed before they can exceed
/// this native capacity, so contact scratch space should remain inline too.
pub(crate) const BOX2D_MAX_POLYGON_VERTICES: usize = 8;
pub(crate) type NativePolygon<T> = SmallVec<[T; BOX2D_MAX_POLYGON_VERTICES]>;

#[cfg(test)]
pub(crate) use circle::{circle_circle_manifold, circle_polygon_manifold};
pub(crate) use circle::{
    circle_circle_manifold_at_transforms, circle_polygon_manifold_at_transforms,
};
pub(crate) use edge::circle_segment_manifold;
#[cfg(test)]
pub(crate) use edge::polygon_segment_manifold;
pub(crate) use edge::polygon_segment_manifold_at_transforms;
#[cfg(test)]
pub(crate) use geometry::dot_2d;
pub(crate) use geometry::{
    closest_point_on_segment, closest_segment_points, polygon_contains_point,
    polygon_segment_core_distance,
};
#[cfg(test)]
pub(crate) use polygon::polygon_manifold;
pub(crate) use polygon::polygon_manifold_at_transforms;
#[cfg(test)]
pub(crate) use polygon::polygon_max_separation;
