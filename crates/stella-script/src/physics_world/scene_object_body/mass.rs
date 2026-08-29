//! Shape ComputeMass calls and float32 fixture-list aggregation.

use crate::*;

impl SceneObject {
    pub(crate) fn native_fixture_mass_data_f32(&self) -> (f32, (f32, f32), f32) {
        self.fixture_mass_data
    }

    /// Aggregate fixture mass data in the exact ResetMassData traversal order.
    /// Fixture vectors retain creation order, so the intrusive native list is
    /// visited in reverse.
    pub(crate) fn compute_native_fixture_mass_data_f32(&self) -> (f32, (f32, f32), f32) {
        let mut mass = 0.0_f32;
        let mut weighted_center = (0.0_f32, 0.0_f32);
        let mut inertia_about_origin = 0.0_f32;
        let mut add_mass_data = |fixture_mass: f32, center: (f32, f32), fixture_inertia: f32| {
            mass += fixture_mass;
            weighted_center.0 = fixture_mass.mul_add(center.0, weighted_center.0);
            weighted_center.1 = fixture_mass.mul_add(center.1, weighted_center.1);
            inertia_about_origin += fixture_inertia;
        };
        let mut add_polygon = |vertices: &[(f64, f64)], density: f32| {
            if let Some((fixture_mass, center, inertia)) =
                native_polygon_mass_data_f32(vertices, density)
            {
                add_mass_data(fixture_mass, center, inertia);
            }
        };
        match &self.collision_shape {
            CollisionShape::Circle { radius } => {
                let radius = (radius.abs() as f32) * (self.physics_scale_x.abs() as f32);
                let radius_squared = radius * radius;
                let fixture_mass =
                    (self.fixture_density(0) as f32) * std::f32::consts::PI * radius_squared;
                add_mass_data(
                    fixture_mass,
                    (0.0, 0.0),
                    fixture_mass * radius_squared.mul_add(0.5_f32, 0.0),
                );
            }
            CollisionShape::Box { width, height } => {
                let half_width = (*width as f32) * (self.physics_scale_x as f32) * 0.5_f32;
                let half_height = (*height as f32) * (self.physics_scale_y as f32) * 0.5_f32;
                add_polygon(
                    &[
                        (f64::from(-half_width), f64::from(-half_height)),
                        (f64::from(half_width), f64::from(-half_height)),
                        (f64::from(half_width), f64::from(half_height)),
                        (f64::from(-half_width), f64::from(half_height)),
                    ],
                    self.fixture_density(0) as f32,
                );
            }
            CollisionShape::Polygon { fixtures, .. } => {
                for fixture in (0..fixtures.len()).rev() {
                    let vertices = &fixtures[fixture];
                    let scaled = vertices
                        .iter()
                        .map(|&(x, y)| {
                            (
                                f64::from((x as f32) * self.physics_scale_x as f32),
                                f64::from((y as f32) * self.physics_scale_y as f32),
                            )
                        })
                        .collect::<Vec<_>>();
                    add_polygon(&scaled, self.fixture_density(fixture) as f32);
                }
            }
            CollisionShape::Line { .. } | CollisionShape::None => {}
        }
        if mass <= 0.0_f32 {
            return (mass, (0.0, 0.0), 0.0);
        }
        // ResetMassData (0x10086B2C4..0x10086B2DC) computes the reciprocal
        // once and multiplies both weighted-center lanes by it.
        let inverse_mass = 1.0_f32 / mass;
        let center = (
            weighted_center.0 * inverse_mass,
            weighted_center.1 * inverse_mass,
        );
        let inertia = if inertia_about_origin > 0.0_f32 && !self.fixed_rotation {
            // 0x10086B314..0x10086B31C is FMUL(y,y), FNMADD(x,x,y²),
            // FMADD(-|c|²,mass,I). Keep both fused operations and the native
            // pre-correction positivity test.
            let negative_center_squared = (-center.0).mul_add(center.0, -(center.1 * center.1));
            negative_center_squared.mul_add(mass, inertia_about_origin)
        } else {
            0.0_f32
        };
        (mass, center, inertia)
    }

    #[cfg(test)]
    pub(crate) fn native_fixture_mass_data(&self) -> (f64, (f64, f64), f64) {
        let (mass, center, inertia) = self.native_fixture_mass_data_f32();
        (
            f64::from(mass),
            (f64::from(center.0), f64::from(center.1)),
            f64::from(inertia),
        )
    }
}

fn native_polygon_mass_data_f32(
    vertices: &[(f64, f64)],
    density: f32,
) -> Option<(f32, (f32, f32), f32)> {
    if density == 0.0_f32 || vertices.len() < 3 {
        return None;
    }
    // b2PolygonShape::ComputeMass (0x10085E10C) accumulates the vertex sum
    // first with V1.2S FADD, then applies one reciprocal multiply. Averaging
    // every vertex separately is equivalent in real arithmetic but not f32.
    let mut reference = (0.0_f32, 0.0_f32);
    for &(x, y) in vertices {
        reference.0 += x as f32;
        reference.1 += y as f32;
    }
    let inverse_count = 1.0_f32 / vertices.len() as f32;
    reference.0 *= inverse_count;
    reference.1 *= inverse_count;

    let mut area = 0.0_f32;
    let mut center = (0.0_f32, 0.0_f32);
    let mut inertia = 0.0_f32;
    let inverse_six = f32::from_bits(0x3E2A_AAAB);
    let inverse_twelve = f32::from_bits(0x3DAA_AAAB);
    for index in 0..vertices.len() {
        let first = (
            vertices[index].0 as f32 - reference.0,
            vertices[index].1 as f32 - reference.1,
        );
        let second_vertex = vertices[(index + 1) % vertices.len()];
        let second = (
            second_vertex.0 as f32 - reference.0,
            second_vertex.1 as f32 - reference.1,
        );
        let cross = (-first.1).mul_add(second.0, first.0 * second.1);
        area = cross.mul_add(0.5_f32, area);
        let center_scale = cross * inverse_six;
        center.0 = center_scale.mul_add(first.0 + second.0, center.0);
        center.1 = center_scale.mul_add(first.1 + second.1, center.1);

        // The native compiler rounds a * (a + b) before fusing b*b, rather
        // than rewriting that first term as an FMA.
        let first_x_sum = first.0 * (first.0 + second.0);
        let integral_x = second.0.mul_add(second.0, first_x_sum);
        let first_y_sum = first.1 * (first.1 + second.1);
        let integral_y = second.1.mul_add(second.1, first_y_sum);
        inertia = (cross * inverse_twelve).mul_add(integral_x + integral_y, inertia);
    }
    if area == 0.0_f32 {
        return None;
    }
    let inverse_area = 1.0_f32 / area;
    let relative_center = (center.0 * inverse_area, center.1 * inverse_area);
    center.0 = reference.0 + relative_center.0;
    center.1 = reference.1 + relative_center.1;
    let fixture_mass = density * area;
    inertia *= density;
    let center_squared = center.0.mul_add(center.0, center.1 * center.1);
    let relative_center_squared = relative_center
        .0
        .mul_add(relative_center.0, relative_center.1 * relative_center.1);
    inertia = fixture_mass.mul_add(center_squared - relative_center_squared, inertia);
    Some((fixture_mass, center, inertia))
}

#[cfg(test)]
mod tests {
    use super::native_polygon_mass_data_f32;

    #[test]
    fn polygon_mass_data_keeps_native_fadd_fmadd_rounding() {
        let vertices = [
            (-17_647.839_843_75, -25_858.328_125),
            (20_172.408_203_125, 20_781.115_234_375),
            (-24_089.597_656_25, 16_789.595_703_125),
        ];
        let (mass, center, inertia) = native_polygon_mass_data_f32(&vertices, 1.0).unwrap();

        assert_eq!(mass.to_bits(), 0x4E64_182E);
        assert_eq!(center.0.to_bits(), 0xC5E0_A2BE);
        assert_eq!(center.1.to_bits(), 0x4574_020C);
        assert_eq!(inertia.to_bits(), 0x5C68_7D9B);
    }
}
