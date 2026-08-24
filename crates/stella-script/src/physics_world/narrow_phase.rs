//! Box2D narrow phase split along Purple's recovered collision members.

mod circle;
mod edge;
mod geometry;
mod polygon;

pub(crate) use circle::{circle_circle_manifold, circle_polygon_manifold};
pub(crate) use edge::{circle_segment_manifold, polygon_segment_manifold};
#[cfg(test)]
pub(crate) use geometry::dot_2d;
pub(crate) use geometry::{
    closest_point_on_segment, closest_segment_points, polygon_contains_point,
    polygon_segment_core_distance,
};
pub(crate) use polygon::polygon_manifold;
#[cfg(test)]
pub(crate) use polygon::polygon_max_separation;
