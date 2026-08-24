//! Per-shape ray-cast result generation used by GameLua and extensions.

use crate::*;

impl SceneObject {
    pub(crate) fn ray_cast_hits(
        &self,
        name: &str,
        start: (f64, f64),
        end: (f64, f64),
    ) -> Vec<RayHit> {
        let ray = (end.0 - start.0, end.1 - start.1);
        let ray_length_squared = ray.0 * ray.0 + ray.1 * ray.1;
        if !ray_length_squared.is_finite() || ray_length_squared <= f64::EPSILON {
            return Vec::new();
        }

        if let Some((_, radius)) = self.collision_circle() {
            let offset = (start.0 - self.x, start.1 - self.y);
            let b = offset.0 * ray.0 + offset.1 * ray.1;
            let c = offset.0 * offset.0 + offset.1 * offset.1 - radius * radius;
            let discriminant = b * b - ray_length_squared * c;
            if c < 0.0 || discriminant < 0.0 || !discriminant.is_finite() {
                return Vec::new();
            }
            let fraction = (-b - discriminant.sqrt()) / ray_length_squared;
            if !(0.0..=1.0).contains(&fraction) {
                return Vec::new();
            }
            let point_x = start.0 + ray.0 * fraction;
            let point_y = start.1 + ray.1 * fraction;
            let normal_length = ((point_x - self.x).powi(2) + (point_y - self.y).powi(2))
                .sqrt()
                .max(f64::EPSILON);
            let hit = RayHit {
                name: name.to_owned(),
                point_x,
                point_y,
                normal_x: (point_x - self.x) / normal_length,
                normal_y: (point_y - self.y) / normal_length,
                fraction,
            };
            return vec![hit];
        }

        let mut hits = self
            .collision_polygons()
            .into_iter()
            .filter_map(|vertices| ray_cast_fixture(name, start, end, &vertices, true))
            .collect::<Vec<_>>();
        hits.extend(self.collision_segments().into_iter().filter_map(|segment| {
            ray_cast_fixture(name, start, end, &[segment.0, segment.1], false)
        }));
        hits
    }
}
