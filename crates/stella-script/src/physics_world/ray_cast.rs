//! Box2D-style fixture ray casting and proxy AABB construction.

use crate::*;

#[derive(Debug, Clone)]
pub(crate) struct RayHit {
    pub(crate) name: String,
    pub(crate) point_x: f64,
    pub(crate) point_y: f64,
    pub(crate) normal_x: f64,
    pub(crate) normal_y: f64,
    pub(crate) fraction: f64,
}

pub(crate) fn ray_cast_fixture(
    name: &str,
    start: (f64, f64),
    end: (f64, f64),
    vertices: &[(f64, f64)],
    close_shape: bool,
) -> Option<RayHit> {
    if vertices.len() < 2 || close_shape && polygon_contains_point(vertices, start) {
        // b2PolygonShape::RayCast reports an entering face only. A ray that
        // begins inside a polygon has no entering face and therefore no hit.
        return None;
    }
    let ray = (end.0 - start.0, end.1 - start.1);
    let edge_count = if close_shape {
        vertices.len()
    } else {
        vertices.len() - 1
    };
    let mut closest: Option<RayHit> = None;
    for index in 0..edge_count {
        let first = vertices[index];
        let second = vertices[(index + 1) % vertices.len()];
        let edge = (second.0 - first.0, second.1 - first.1);
        let denominator = cross_2d(ray, edge);
        if denominator.abs() <= f64::EPSILON {
            continue;
        }
        let relative = (first.0 - start.0, first.1 - start.1);
        let fraction = cross_2d(relative, edge) / denominator;
        let edge_fraction = cross_2d(relative, ray) / denominator;
        if !(0.0..=1.0).contains(&fraction) || !(0.0..=1.0).contains(&edge_fraction) {
            continue;
        }
        if closest.as_ref().is_some_and(|hit| fraction >= hit.fraction) {
            continue;
        }
        let normal_length = (edge.0 * edge.0 + edge.1 * edge.1).sqrt().max(f64::EPSILON);
        let mut normal = (-edge.1 / normal_length, edge.0 / normal_length);
        if normal.0 * ray.0 + normal.1 * ray.1 > 0.0 {
            normal = (-normal.0, -normal.1);
        }
        closest = Some(RayHit {
            name: name.to_owned(),
            point_x: start.0 + ray.0 * fraction,
            point_y: start.1 + ray.1 * fraction,
            normal_x: normal.0,
            normal_y: normal.1,
            fraction,
        });
    }
    closest
}
