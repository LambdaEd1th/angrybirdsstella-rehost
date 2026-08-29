//! Data materialized by Purple's `b2PolygonShape::Set` (`0x10085DB9C`).

/// Recreate the exact local normal array stored by `b2PolygonShape::Set`.
/// Purple neither repairs winding nor rejects a zero, sub-epsilon or NaN
/// edge. Its `FCMP`/`B.LT` branches around reciprocal normalization for a
/// sub-epsilon or unordered length.
pub(crate) fn native_polygon_normals(vertices: &[(f32, f32)]) -> Vec<(f32, f32)> {
    vertices
        .iter()
        .copied()
        .zip(vertices.iter().copied().cycle().skip(1))
        .take(vertices.len())
        .map(|(first, second)| {
            let edge = (second.0 - first.0, second.1 - first.1);
            let mut normal = (edge.1, -edge.0);
            let length = edge.0.mul_add(edge.0, edge.1 * edge.1).sqrt();
            if length >= f32::EPSILON {
                let inverse_length = 1.0_f32 / length;
                normal.0 *= inverse_length;
                normal.1 *= inverse_length;
            }
            normal
        })
        .collect()
}

/// Recreate the centroid stored at `b2PolygonShape+0x10`.
///
/// `0x10085DC74..0x10085DCE0` accumulates signed triangle area and first
/// moments directly in source coordinates. It does not use a reference point
/// or reject a zero, sub-epsilon or NaN area before the final reciprocal.
pub(crate) fn native_polygon_centroid_f32(vertices: &[(f32, f32)]) -> (f32, f32) {
    let mut area = 0.0_f32;
    let mut center = (0.0_f32, 0.0_f32);
    let inverse_six = f32::from_bits(0x3E2A_AAAB);

    for (&first, &second) in vertices
        .iter()
        .zip(vertices.iter().cycle().skip(1))
        .take(vertices.len())
    {
        // FNMSUB at 0x10085DC98 consumes the already-rounded y1*x2 product.
        let cross = first.0.mul_add(second.1, -(first.1 * second.0));
        area = cross.mul_add(0.5_f32, area);
        let center_scale = cross * inverse_six;
        center.0 = (first.0 + second.0).mul_add(center_scale, center.0);
        center.1 = (first.1 + second.1).mul_add(center_scale, center.1);
    }

    let inverse_area = 1.0_f32 / area;
    (center.0 * inverse_area, center.1 * inverse_area)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn polygon_normals_keep_setters_signed_vertex_order() {
        let clockwise = [(-1.0, -1.0), (-1.0, 1.0), (1.0, 1.0), (1.0, -1.0)];
        assert_eq!(
            native_polygon_normals(&clockwise),
            vec![(1.0, -0.0), (0.0, -1.0), (-1.0, -0.0), (0.0, 1.0)]
        );
    }

    #[test]
    fn polygon_set_preserves_an_unordered_raw_normal() {
        let normals = native_polygon_normals(&[(0.0, 0.0), (f32::NAN, 0.0), (0.0, 1.0)]);

        assert_eq!(normals[0].0.to_bits(), 0.0_f32.to_bits());
        assert!(normals[0].1.is_nan());
    }

    #[test]
    fn polygon_set_divides_an_empty_centroid_by_zero_area() {
        let centroid = native_polygon_centroid_f32(&[]);

        assert!(centroid.0.is_nan());
        assert!(centroid.1.is_nan());
    }

    #[test]
    fn polygon_set_centroid_keeps_native_area_and_first_moment_rounding() {
        let vertices = [
            (f32::from_bits(0xC689_DFAE), f32::from_bits(0xC6CA_04A8)),
            (f32::from_bits(0x469D_98D1), f32::from_bits(0x46A2_5A3B)),
            (f32::from_bits(0xC6BC_3332), f32::from_bits(0x4683_2B31)),
        ];
        let centroid = native_polygon_centroid_f32(&vertices);

        let mut regrouped_area = 0.0_f32;
        let mut regrouped = (0.0_f32, 0.0_f32);
        for (&first, &second) in vertices
            .iter()
            .zip(vertices.iter().cycle().skip(1))
            .take(vertices.len())
        {
            let cross = first.0.mul_add(second.1, -(first.1 * second.0));
            regrouped_area += cross;
            regrouped.0 = (first.0 + second.0).mul_add(cross, regrouped.0);
            regrouped.1 = (first.1 + second.1).mul_add(cross, regrouped.1);
        }
        let regrouped_scale = (3.0_f32 * regrouped_area).recip();
        regrouped.0 *= regrouped_scale;
        regrouped.1 *= regrouped_scale;

        assert_eq!(centroid.0.to_bits(), 0xC5E0_A2BF);
        assert_eq!(centroid.1.to_bits(), 0x4574_020C);
        assert_eq!(regrouped.0.to_bits(), 0xC5E0_A2BF);
        assert_eq!(regrouped.1.to_bits(), 0x4574_020B);
    }
}
