//! Object and per-fixture Box2D bounds.

use crate::*;

impl SceneObject {
    #[cfg(test)]
    pub(crate) fn collision_aabb(&self) -> Option<(f64, f64, f64, f64)> {
        if ![
            self.x,
            self.y,
            self.physics_scale_x,
            self.physics_scale_y,
            self.angle,
        ]
        .into_iter()
        .all(f64::is_finite)
        {
            return None;
        }
        if let Some((center, radius)) = self.collision_circle() {
            let bounds = native_circle_aabb(center, radius);
            return Some(tuple_f64(bounds));
        }
        let vertices = self.collision_vertices();
        native_fixture_aabb(&vertices, BOX2D_POLYGON_RADIUS as f32).map(tuple_f64)
    }

    pub(crate) fn collision_fixture_aabbs(&self) -> Vec<(f32, f32, f32, f32)> {
        if let Some((center, radius)) = self.collision_circle() {
            return vec![native_circle_aabb(center, radius)];
        }
        let polygon_aabbs = self
            .collision_polygons()
            .into_iter()
            .filter_map(|vertices| native_fixture_aabb(&vertices, BOX2D_POLYGON_RADIUS as f32))
            .collect::<Vec<_>>();
        if !polygon_aabbs.is_empty() {
            return polygon_aabbs;
        }
        self.collision_segments()
            .into_iter()
            .filter_map(|segment| {
                native_fixture_aabb(&[segment.0, segment.1], BOX2D_POLYGON_RADIUS as f32)
            })
            .collect()
    }
}

fn native_circle_aabb(center: (f64, f64), radius: f64) -> (f32, f32, f32, f32) {
    let center = (center.0 as f32, center.1 as f32);
    let radius = radius as f32;
    (
        center.0 - radius,
        center.1 - radius,
        center.0 + radius,
        center.1 + radius,
    )
}

fn native_fixture_aabb(vertices: &[(f64, f64)], radius: f32) -> Option<(f32, f32, f32, f32)> {
    let &(first_x, first_y) = vertices.first()?;
    let (mut left, mut down, mut right, mut up) = (
        first_x as f32,
        first_y as f32,
        first_x as f32,
        first_y as f32,
    );
    for &(x, y) in &vertices[1..] {
        let (x, y) = (x as f32, y as f32);
        left = left.min(x);
        down = down.min(y);
        right = right.max(x);
        up = up.max(y);
    }
    Some((left - radius, down - radius, right + radius, up + radius))
}

#[cfg(test)]
fn tuple_f64(bounds: (f32, f32, f32, f32)) -> (f64, f64, f64, f64) {
    (
        f64::from(bounds.0),
        f64::from(bounds.1),
        f64::from(bounds.2),
        f64::from(bounds.3),
    )
}

#[cfg(test)]
mod tests {
    use super::native_fixture_aabb;

    #[test]
    fn fixture_skin_is_added_in_native_float32_precision() {
        let x = f32::from_bits(0x2dbc_7c70);
        let bounds = native_fixture_aabb(&[(f64::from(x), 0.0)], 0.002_f32).unwrap();

        assert_eq!(bounds.0.to_bits(), (x - 0.002_f32).to_bits());
        assert_ne!(
            bounds.0.to_bits(),
            ((f64::from(x) - 0.002_f64) as f32).to_bits()
        );
    }
}
