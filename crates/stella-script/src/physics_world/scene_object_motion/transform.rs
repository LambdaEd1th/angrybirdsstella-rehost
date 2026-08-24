//! Float32 `b2Transform` projection for body-local points.

use crate::*;

impl SceneObject {
    /// Transform a point already expressed in the native body's local frame.
    /// Fixture rebuild scaling is deliberately absent: Box2D joint anchors,
    /// sweep centres and stored contact witnesses all use this coordinate
    /// space after shape scaling has already happened.
    pub(crate) fn native_transform_body_point(&self, point: (f32, f32)) -> (f32, f32) {
        let (sine, cosine) = (self.angle as f32).sin_cos();
        (
            point
                .0
                .mul_add(cosine, (-point.1).mul_add(sine, self.x as f32)),
            point
                .0
                .mul_add(sine, point.1.mul_add(cosine, self.y as f32)),
        )
    }

    /// Purple inlines `b2MulT(transform, worldPoint)` when constructing
    /// joints and position constraints. Inputs and every arithmetic result
    /// are float32; `physicsScale` belongs to fixture creation, not b2Body.
    pub(crate) fn native_inverse_transform_body_point(&self, point: (f64, f64)) -> (f64, f64) {
        let delta_x = point.0 as f32 - self.x as f32;
        let delta_y = point.1 as f32 - self.y as f32;
        let (sine, cosine) = (self.angle as f32).sin_cos();
        (
            f64::from(delta_y.mul_add(sine, delta_x * cosine)),
            f64::from((-delta_x).mul_add(sine, delta_y * cosine)),
        )
    }
}
