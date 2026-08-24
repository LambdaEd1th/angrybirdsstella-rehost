//! Purple's float32 `b2Distance`/GJK query.

use super::{
    NativeDistanceProxy, NativeToiTransform, native_toi_dot, native_toi_sub,
    simplex::{
        NativeSimplexCache, NativeSimplexVertex, native_simplex_search_direction,
        native_simplex_solve_three, native_simplex_solve_two,
    },
};

/// `b2Distance` (`sub_1008605D4`) with `useRadii=false`, as called by
/// Purple's TOI implementation. The simplex cache is returned solely to seed
/// the native separation-function type and feature indices.
pub(crate) fn native_core_distance(
    proxy_a: &NativeDistanceProxy,
    transform_a: NativeToiTransform,
    proxy_b: &NativeDistanceProxy,
    transform_b: NativeToiTransform,
) -> (f32, NativeSimplexCache) {
    let mut vertices = [NativeSimplexVertex::default(); 3];
    let mut count = 1;
    let index_a = 0;
    let index_b = 0;
    let point_a = transform_a.point(proxy_a.vertices[index_a]);
    let point_b = transform_b.point(proxy_b.vertices[index_b]);
    vertices[0] = NativeSimplexVertex {
        point_a,
        point_b,
        difference: native_toi_sub(point_b, point_a),
        weight: 1.0_f32,
        index_a,
        index_b,
    };

    for _ in 0..20 {
        let saved = vertices[..count]
            .iter()
            .map(|vertex| (vertex.index_a, vertex.index_b))
            .collect::<Vec<_>>();
        count = match count {
            2 => native_simplex_solve_two(&mut vertices),
            3 => native_simplex_solve_three(&mut vertices),
            _ => count,
        };
        if count == 3 {
            break;
        }
        let direction = native_simplex_search_direction(&vertices, count);
        if native_toi_dot(direction, direction) < f32::EPSILON * f32::EPSILON {
            break;
        }
        let direction_a = transform_a.inverse_rotate((-direction.0, -direction.1));
        let direction_b = transform_b.inverse_rotate(direction);
        let index_a = proxy_a.support(direction_a);
        let index_b = proxy_b.support(direction_b);
        if saved
            .iter()
            .any(|&(saved_a, saved_b)| saved_a == index_a && saved_b == index_b)
        {
            break;
        }
        let point_a = transform_a.point(proxy_a.vertices[index_a]);
        let point_b = transform_b.point(proxy_b.vertices[index_b]);
        vertices[count] = NativeSimplexVertex {
            point_a,
            point_b,
            difference: native_toi_sub(point_b, point_a),
            weight: 0.0_f32,
            index_a,
            index_b,
        };
        count += 1;
    }

    count = match count {
        2 => native_simplex_solve_two(&mut vertices),
        3 => native_simplex_solve_three(&mut vertices),
        _ => count,
    };
    let (witness_a, witness_b) = match count {
        1 => (vertices[0].point_a, vertices[0].point_b),
        2 => (
            (
                vertices[0].weight.mul_add(
                    vertices[0].point_a.0,
                    vertices[1].weight * vertices[1].point_a.0,
                ),
                vertices[0].weight.mul_add(
                    vertices[0].point_a.1,
                    vertices[1].weight * vertices[1].point_a.1,
                ),
            ),
            (
                vertices[0].weight.mul_add(
                    vertices[0].point_b.0,
                    vertices[1].weight * vertices[1].point_b.0,
                ),
                vertices[0].weight.mul_add(
                    vertices[0].point_b.1,
                    vertices[1].weight * vertices[1].point_b.1,
                ),
            ),
        ),
        _ => {
            let point = (
                vertices[0].weight.mul_add(
                    vertices[0].point_a.0,
                    vertices[1].weight.mul_add(
                        vertices[1].point_a.0,
                        vertices[2].weight * vertices[2].point_a.0,
                    ),
                ),
                vertices[0].weight.mul_add(
                    vertices[0].point_a.1,
                    vertices[1].weight.mul_add(
                        vertices[1].point_a.1,
                        vertices[2].weight * vertices[2].point_a.1,
                    ),
                ),
            );
            (point, point)
        }
    };
    let delta = native_toi_sub(witness_b, witness_a);
    let mut cache = NativeSimplexCache {
        count,
        ..NativeSimplexCache::default()
    };
    for (index, vertex) in vertices.iter().take(count).enumerate() {
        cache.index_a[index] = vertex.index_a;
        cache.index_b[index] = vertex.index_b;
    }
    (native_toi_dot(delta, delta).sqrt(), cache)
}
