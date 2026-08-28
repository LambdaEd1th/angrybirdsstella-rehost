//! Persistent local witnesses stored by `b2ContactPositionConstraint`.

use crate::SceneObject;

fn native_negative_rotated_x(point: (f32, f32), sine: f32, cosine: f32) -> f32 {
    point.1.mul_add(sine, -(point.0 * cosine))
}

/// The compact b2Position array state consumed by contact constraints. The
/// native solver does not copy full render objects into every point pass.
#[derive(Debug, Clone, Copy)]
pub(crate) struct PositionBodyState {
    pub(crate) center: (f32, f32),
    pub(crate) angle: f32,
}

impl PositionBodyState {
    pub(crate) fn capture(object: &SceneObject) -> Self {
        Self {
            center: object.native_world_center(),
            angle: object.angle as f32,
        }
    }

    fn transform_components(self, local_center: (f32, f32)) -> ((f32, f32), f32, f32) {
        let (sine, cosine) = self.angle.sin_cos();
        // sub_1008647DC/804 rounds the first product before the fused second
        // leg for both coordinates of R * localCenter.
        let rotated_x = (-local_center.1).mul_add(sine, local_center.0 * cosine);
        let rotated_y = local_center.1.mul_add(cosine, local_center.0 * sine);
        let position = (self.center.0 - rotated_x, self.center.1 - rotated_y);
        (position, sine, cosine)
    }

    pub(crate) fn transform_point(self, local_center: (f32, f32), point: (f32, f32)) -> (f32, f32) {
        let (position, sine, cosine) = self.transform_components(local_center);
        // PositionSolverManifold rotates the local witness first and performs
        // a separate FADD with transform.p; translation is not nested in the
        // rotation FMADD chain.
        let rotated_x = point.0.mul_add(cosine, -(point.1 * sine));
        let rotated_y = point.0.mul_add(sine, point.1 * cosine);
        (position.0 + rotated_x, position.1 + rotated_y)
    }

    /// Face manifolds keep the reference plane's x coordinate negated. The
    /// leaf rounds `point.x * cosine`, fuses `point.y * sine - product`, then
    /// subtracts transform.p.x. Its y coordinate remains an ordinary world
    /// transform.
    pub(crate) fn negative_plane_x_and_world_y(
        self,
        local_center: (f32, f32),
        point: (f32, f32),
    ) -> (f32, f32) {
        let (position, sine, cosine) = self.transform_components(local_center);
        let negative_rotated_x = native_negative_rotated_x(point, sine, cosine);
        let rotated_y = point.0.mul_add(sine, point.1 * cosine);
        (negative_rotated_x - position.0, position.1 + rotated_y)
    }
}

#[derive(Debug, Clone)]
pub(crate) struct PositionContactConstraint {
    pub(crate) first_local_center: (f32, f32),
    pub(crate) second_local_center: (f32, f32),
    pub(crate) first_inverse_mass: f32,
    pub(crate) second_inverse_mass: f32,
    pub(crate) first_inverse_inertia: f32,
    pub(crate) second_inverse_inertia: f32,
    pub(crate) manifold: PositionContactManifold,
}

#[derive(Debug, Clone)]
pub(crate) enum PositionContactManifold {
    Circles {
        local_first: (f64, f64),
        local_second: (f64, f64),
        first_radius: f64,
        second_radius: f64,
    },
    FaceFirst {
        local_normal: (f64, f64),
        local_plane_point: (f64, f64),
        local_clip_points: Vec<(f64, f64)>,
        first_radius: f64,
        second_radius: f64,
    },
    FaceSecond {
        local_normal: (f64, f64),
        local_plane_point: (f64, f64),
        local_clip_points: Vec<(f64, f64)>,
        first_radius: f64,
        second_radius: f64,
    },
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct PositionWorldPoint {
    pub(crate) normal: (f32, f32),
    pub(crate) point: (f32, f32),
    pub(crate) separation: f32,
}

#[cfg(test)]
mod tests {
    use super::{PositionBodyState, native_negative_rotated_x};

    #[test]
    fn local_center_transform_rounds_first_products_before_native_fmadds() {
        let body = PositionBodyState {
            center: (f32::from_bits(0x3F05_330A), f32::from_bits(0xBF02_86E1)),
            angle: f32::from_bits(0x3E1A_C320),
        };
        let local_center = (f32::from_bits(0x3F2C_0FBB), f32::from_bits(0xBF33_41DE));
        let native = body.transform_point(local_center, (0.0, 0.0));
        assert_eq!(native.0.to_bits(), 0xBE7F_8F1C);
        assert_eq!(native.1.to_bits(), 0x3DA6_4058);

        let (sine, cosine) = body.angle.sin_cos();
        let fused_first_products = (
            body.center.0 - local_center.0.mul_add(cosine, -(local_center.1 * sine)),
            body.center.1 - local_center.0.mul_add(sine, local_center.1 * cosine),
        );
        assert_eq!(fused_first_products.0.to_bits(), 0xBE7F_8F18);
        assert_eq!(fused_first_products.1.to_bits(), 0x3DA6_4060);

        let point = (f32::from_bits(0x3F13_7A5C), f32::from_bits(0xBF2B_918E));
        let transformed = body.transform_point(local_center, point);
        let nested_translation = (
            point.0.mul_add(cosine, (-point.1).mul_add(sine, native.0)),
            point.0.mul_add(sine, point.1.mul_add(cosine, native.1)),
        );
        assert_eq!(transformed.0.to_bits(), 0x3ED7_7A1C);
        assert_eq!(transformed.1.to_bits(), 0xBEFD_4114);
        assert_eq!(nested_translation.0.to_bits(), 0x3ED7_7A1C);
        assert_eq!(nested_translation.1.to_bits(), 0xBEFD_4113);
    }

    #[test]
    fn face_plane_x_keeps_the_native_negated_transform_grouping() {
        let body = PositionBodyState {
            center: (0.0, 0.0),
            angle: 0.0,
        };
        let point_x = f32::from_bits(0x4229_6D75);
        let point_y = f32::from_bits(0xC286_F8A3);
        let sine = f32::from_bits(0xC2C1_8DD3);
        let cosine = f32::from_bits(0x4261_5B7E);
        let native_negative_x = native_negative_rotated_x((point_x, point_y), sine, cosine);
        let generic_negative_x = -point_x.mul_add(cosine, -(point_y * sine));
        assert_eq!(native_negative_x.to_bits(), 0x4581_8592);
        assert_eq!(generic_negative_x.to_bits(), 0x4581_8591);

        assert_eq!(
            body.negative_plane_x_and_world_y((0.0, 0.0), (point_x, point_y)),
            (-point_x, point_y)
        );
    }
}
