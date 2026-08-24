//! Native polygon decomposition facade.

mod geometry;
mod merge;
mod triangulation;

pub(crate) type NativePoint = (f32, f32);
pub(crate) type PolygonFixtures = (Vec<(f64, f64)>, Vec<Vec<(f64, f64)>>);

#[cfg(test)]
pub(crate) fn polygon_area(vertices: &[(f64, f64)]) -> f64 {
    geometry::polygon_area(vertices)
}

/// Prepare the exact contour and fixture list consumed by the polygon creator
/// at sub_100068310. Seven or fewer convex vertices use the direct Box2D path;
/// everything else enters the recovered decomposition pipeline.
pub(crate) fn native_polygon_fixtures(vertices: &[(f64, f64)]) -> PolygonFixtures {
    let vertices = vertices
        .iter()
        .map(|&(x, y)| (f64::from(x as f32), f64::from(y as f32)))
        .collect::<Vec<_>>();
    if vertices.len() < 3 {
        return (vertices, Vec::new());
    }
    let native = vertices
        .iter()
        .map(|&(x, y)| (x as f32, y as f32))
        .collect::<Vec<_>>();
    let fixtures = if native.len() <= 7 && geometry::native_polygon_is_convex(&native) {
        vec![vertices.clone()]
    } else {
        decompose_native_polygon(&vertices)
    };
    (vertices, fixtures)
}

/// Purple's sub_100872360 pipeline: winding-normalized ear cutting followed
/// by the <=8-point convex merger at sub_100871F08.
pub(crate) fn decompose_native_polygon(vertices: &[(f64, f64)]) -> Vec<Vec<(f64, f64)>> {
    merge::native_merge_triangles(&native_triangulate_polygon(vertices))
        .into_iter()
        .map(|polygon| {
            polygon
                .into_iter()
                .map(|(x, y)| (f64::from(x), f64::from(y)))
                .collect()
        })
        .collect()
}

/// DrawablePolygon and the Box2D bridge share the float32 winding normalizer
/// and quality-ranked ear cutter.
pub(crate) fn native_triangulate_polygon(vertices: &[(f64, f64)]) -> Vec<[NativePoint; 3]> {
    triangulation::native_triangulate_polygon(vertices)
}

pub(crate) fn native_triangulate_clockwise(
    contour: Vec<NativePoint>,
) -> Option<Vec<[NativePoint; 3]>> {
    triangulation::native_triangulate_clockwise(contour)
}
