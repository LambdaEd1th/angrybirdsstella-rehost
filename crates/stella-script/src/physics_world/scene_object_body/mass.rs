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
        if !self.dynamic_body {
            return (0.0, (0.0, 0.0), 0.0);
        }
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
            if density == 0.0_f32 || vertices.len() < 3 {
                return;
            }
            let inverse_count = 1.0_f32 / vertices.len() as f32;
            let mut reference = (0.0_f32, 0.0_f32);
            for &(x, y) in vertices {
                reference.0 += inverse_count * x as f32;
                reference.1 += inverse_count * y as f32;
            }

            let mut area = 0.0_f32;
            let mut center = (0.0_f32, 0.0_f32);
            let mut inertia = 0.0_f32;
            let inverse_three = 1.0_f32 / 3.0_f32;
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
                let triangle_area = 0.5_f32 * cross;
                area += triangle_area;
                let center_scale = triangle_area * inverse_three;
                center.0 = center_scale.mul_add(first.0 + second.0, center.0);
                center.1 = center_scale.mul_add(first.1 + second.1, center.1);

                let first_x_sum = second.0.mul_add(first.0, first.0 * first.0);
                let integral_x = second.0.mul_add(second.0, first_x_sum);
                let first_y_sum = second.1.mul_add(first.1, first.1 * first.1);
                let integral_y = second.1.mul_add(second.1, first_y_sum);
                inertia =
                    (0.25_f32 * inverse_three * cross).mul_add(integral_x + integral_y, inertia);
            }
            if area == 0.0_f32 {
                return;
            }
            center.0 = center.0 / area + reference.0;
            center.1 = center.1 / area + reference.1;
            let fixture_mass = density * area;
            inertia *= density;
            let center_squared = center.0.mul_add(center.0, center.1 * center.1);
            let offset = (center.0 - reference.0, center.1 - reference.1);
            let offset_squared = offset.0.mul_add(offset.0, offset.1 * offset.1);
            inertia = fixture_mass.mul_add(center_squared - offset_squared, inertia);
            add_mass_data(fixture_mass, center, inertia);
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
                    fixture_mass * 0.5_f32 * radius_squared,
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
        let center = (weighted_center.0 / mass, weighted_center.1 / mass);
        let center_squared = center.0.mul_add(center.0, center.1 * center.1);
        let inertia = (inertia_about_origin - mass * center_squared).max(0.0_f32);
        (mass, center, inertia)
    }

    pub(crate) fn native_fixture_mass_data(&self) -> (f64, (f64, f64), f64) {
        let (mass, center, inertia) = self.native_fixture_mass_data_f32();
        (
            f64::from(mass),
            (f64::from(center.0), f64::from(center.1)),
            f64::from(inertia),
        )
    }
}
