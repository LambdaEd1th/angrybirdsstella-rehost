//! DrawablePolygon rebuild and triangle-stream projection for Dirt rendering.

use crate::*;

pub(crate) fn triangulate_dirt_paths(paths: &[Vec<(f64, f64)>]) -> Vec<Vec<RenderTriangle>> {
    paths
        .iter()
        .filter_map(|path| {
            if path.len() < 3 {
                return None;
            }
            // DrawablePolygon::rebuild (`sub_1000246A8`) copies the contour
            // into float32 arrays, reverses it unconditionally and invokes
            // the quality-ranked ear cutter at `sub_100871498`.
            let mut contour = path
                .iter()
                .map(|&(x, y)| (x as f32, y as f32))
                .collect::<Vec<_>>();
            contour.reverse();
            native_triangulate_clockwise(contour).map(|triangles| {
                triangles
                    .into_iter()
                    .map(|triangle| RenderTriangle {
                        vertices: triangle.map(|(x, y)| [f64::from(x), f64::from(y)]),
                    })
                    .collect()
            })
        })
        .collect()
}
