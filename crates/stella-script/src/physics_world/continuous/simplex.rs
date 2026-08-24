//! `b2Simplex` feature cache, region solvers and search direction.

use super::{native_toi_cross, native_toi_dot, native_toi_sub};

#[derive(Debug, Clone, Copy, Default)]
pub(super) struct NativeSimplexVertex {
    pub(super) point_a: (f32, f32),
    pub(super) point_b: (f32, f32),
    pub(super) difference: (f32, f32),
    pub(super) weight: f32,
    pub(super) index_a: usize,
    pub(super) index_b: usize,
}

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct NativeSimplexCache {
    pub(super) count: usize,
    pub(super) index_a: [usize; 3],
    pub(super) index_b: [usize; 3],
}

pub(super) fn native_simplex_solve_two(vertices: &mut [NativeSimplexVertex; 3]) -> usize {
    let edge = native_toi_sub(vertices[1].difference, vertices[0].difference);
    let region_2 = -native_toi_dot(vertices[0].difference, edge);
    if region_2 <= 0.0_f32 {
        vertices[0].weight = 1.0_f32;
        return 1;
    }
    let region_1 = native_toi_dot(vertices[1].difference, edge);
    if region_1 <= 0.0_f32 {
        vertices[1].weight = 1.0_f32;
        vertices[0] = vertices[1];
        return 1;
    }
    let inverse = (region_1 + region_2).recip();
    vertices[0].weight = region_1 * inverse;
    vertices[1].weight = region_2 * inverse;
    2
}

pub(super) fn native_simplex_solve_three(vertices: &mut [NativeSimplexVertex; 3]) -> usize {
    let point_1 = vertices[0].difference;
    let point_2 = vertices[1].difference;
    let point_3 = vertices[2].difference;
    let edge_12 = native_toi_sub(point_2, point_1);
    let edge_13 = native_toi_sub(point_3, point_1);
    let edge_23 = native_toi_sub(point_3, point_2);
    let d12_1 = native_toi_dot(point_2, edge_12);
    let d12_2 = -native_toi_dot(point_1, edge_12);
    let d13_1 = native_toi_dot(point_3, edge_13);
    let d13_2 = -native_toi_dot(point_1, edge_13);
    let d23_1 = native_toi_dot(point_3, edge_23);
    let d23_2 = -native_toi_dot(point_2, edge_23);
    let normal_123 = native_toi_cross(edge_12, edge_13);
    let d123_1 = normal_123 * native_toi_cross(point_2, point_3);
    let d123_2 = normal_123 * native_toi_cross(point_3, point_1);
    let d123_3 = normal_123 * native_toi_cross(point_1, point_2);

    if d12_2 <= 0.0_f32 && d13_2 <= 0.0_f32 {
        vertices[0].weight = 1.0_f32;
        return 1;
    }
    if d12_1 > 0.0_f32 && d12_2 > 0.0_f32 && d123_3 <= 0.0_f32 {
        let inverse = (d12_1 + d12_2).recip();
        vertices[0].weight = d12_1 * inverse;
        vertices[1].weight = d12_2 * inverse;
        return 2;
    }
    if d13_1 > 0.0_f32 && d13_2 > 0.0_f32 && d123_2 <= 0.0_f32 {
        let inverse = (d13_1 + d13_2).recip();
        vertices[0].weight = d13_1 * inverse;
        vertices[2].weight = d13_2 * inverse;
        vertices[1] = vertices[2];
        return 2;
    }
    if d12_1 <= 0.0_f32 && d23_2 <= 0.0_f32 {
        vertices[1].weight = 1.0_f32;
        vertices[0] = vertices[1];
        return 1;
    }
    if d13_1 <= 0.0_f32 && d23_1 <= 0.0_f32 {
        vertices[2].weight = 1.0_f32;
        vertices[0] = vertices[2];
        return 1;
    }
    if d23_1 > 0.0_f32 && d23_2 > 0.0_f32 && d123_1 <= 0.0_f32 {
        let inverse = (d23_1 + d23_2).recip();
        vertices[1].weight = d23_1 * inverse;
        vertices[2].weight = d23_2 * inverse;
        vertices[0] = vertices[1];
        vertices[1] = vertices[2];
        return 2;
    }

    let inverse = (d123_1 + d123_2 + d123_3).recip();
    vertices[0].weight = d123_1 * inverse;
    vertices[1].weight = d123_2 * inverse;
    vertices[2].weight = d123_3 * inverse;
    3
}

pub(super) fn native_simplex_search_direction(
    vertices: &[NativeSimplexVertex; 3],
    count: usize,
) -> (f32, f32) {
    if count == 1 {
        return (-vertices[0].difference.0, -vertices[0].difference.1);
    }
    let edge = native_toi_sub(vertices[1].difference, vertices[0].difference);
    let origin = (-vertices[0].difference.0, -vertices[0].difference.1);
    if native_toi_cross(edge, origin) > 0.0_f32 {
        (-edge.1, edge.0)
    } else {
        (edge.1, -edge.0)
    }
}
