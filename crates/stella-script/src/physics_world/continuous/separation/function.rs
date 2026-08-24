//! Purple's three `b2SeparationFunction` members.

use super::super::{
    NativeDistanceProxy, NativeSweep, native_toi_dot, native_toi_sub, simplex::NativeSimplexCache,
};

const NATIVE_EPSILON: f32 = f32::EPSILON;

#[derive(Debug, Clone, Copy)]
pub(super) enum NativeSeparationFunction {
    Points {
        axis: (f32, f32),
    },
    FaceA {
        axis: (f32, f32),
        local_point: (f32, f32),
    },
    FaceB {
        axis: (f32, f32),
        local_point: (f32, f32),
    },
}

impl NativeSeparationFunction {
    /// `b2SeparationFunction::Initialize` (`sub_1008620E4`).
    pub(super) fn initialize(
        cache: NativeSimplexCache,
        proxy_a: &NativeDistanceProxy,
        sweep_a: NativeSweep,
        proxy_b: &NativeDistanceProxy,
        sweep_b: NativeSweep,
        alpha: f32,
    ) -> Self {
        let transform_a = sweep_a.transform(alpha);
        let transform_b = sweep_b.transform(alpha);
        if cache.count == 1 {
            let point_a = transform_a.point(proxy_a.vertices[cache.index_a[0]]);
            let point_b = transform_b.point(proxy_b.vertices[cache.index_b[0]]);
            let axis = normalize_point_axis(native_toi_sub(point_b, point_a));
            return Self::Points { axis };
        }
        if cache.index_a[0] == cache.index_a[1] {
            let local_1 = proxy_b.vertices[cache.index_b[0]];
            let local_2 = proxy_b.vertices[cache.index_b[1]];
            let edge = native_toi_sub(local_2, local_1);
            let mut axis = normalize_face_axis((edge.1, -edge.0));
            let local_point = (
                (local_1.0 + local_2.0) * 0.5_f32,
                (local_1.1 + local_2.1) * 0.5_f32,
            );
            let normal = transform_b.rotate(axis);
            let point_b = transform_b.point(local_point);
            let point_a = transform_a.point(proxy_a.vertices[cache.index_a[0]]);
            if native_toi_dot(native_toi_sub(point_a, point_b), normal) < 0.0_f32 {
                axis = (-axis.0, -axis.1);
            }
            Self::FaceB { axis, local_point }
        } else {
            let local_1 = proxy_a.vertices[cache.index_a[0]];
            let local_2 = proxy_a.vertices[cache.index_a[1]];
            let edge = native_toi_sub(local_2, local_1);
            let mut axis = normalize_face_axis((edge.1, -edge.0));
            let local_point = (
                (local_1.0 + local_2.0) * 0.5_f32,
                (local_1.1 + local_2.1) * 0.5_f32,
            );
            let normal = transform_a.rotate(axis);
            let point_a = transform_a.point(local_point);
            let point_b = transform_b.point(proxy_b.vertices[cache.index_b[0]]);
            if native_toi_dot(native_toi_sub(point_b, point_a), normal) < 0.0_f32 {
                axis = (-axis.0, -axis.1);
            }
            Self::FaceA { axis, local_point }
        }
    }

    /// `b2SeparationFunction::FindMinSeparation` (`sub_1008624A4`).
    pub(super) fn find_min_separation(
        self,
        proxy_a: &NativeDistanceProxy,
        sweep_a: NativeSweep,
        proxy_b: &NativeDistanceProxy,
        sweep_b: NativeSweep,
        alpha: f32,
    ) -> (f32, usize, usize) {
        let transform_a = sweep_a.transform(alpha);
        let transform_b = sweep_b.transform(alpha);
        match self {
            Self::Points { axis } => {
                let index_a = proxy_a.support(transform_a.inverse_rotate(axis));
                let index_b = proxy_b.support(transform_b.inverse_rotate((-axis.0, -axis.1)));
                let point_a = transform_a.point(proxy_a.vertices[index_a]);
                let point_b = transform_b.point(proxy_b.vertices[index_b]);
                (
                    native_toi_dot(native_toi_sub(point_b, point_a), axis),
                    index_a,
                    index_b,
                )
            }
            Self::FaceA { axis, local_point } => {
                let normal = transform_a.rotate(axis);
                let point_a = transform_a.point(local_point);
                let index_b = proxy_b.support(transform_b.inverse_rotate((-normal.0, -normal.1)));
                let point_b = transform_b.point(proxy_b.vertices[index_b]);
                (
                    native_toi_dot(native_toi_sub(point_b, point_a), normal),
                    0,
                    index_b,
                )
            }
            Self::FaceB { axis, local_point } => {
                let normal = transform_b.rotate(axis);
                let point_b = transform_b.point(local_point);
                let index_a = proxy_a.support(transform_a.inverse_rotate((-normal.0, -normal.1)));
                let point_a = transform_a.point(proxy_a.vertices[index_a]);
                (
                    native_toi_dot(native_toi_sub(point_a, point_b), normal),
                    index_a,
                    0,
                )
            }
        }
    }

    /// `b2SeparationFunction::Evaluate` (`sub_100862898`).
    pub(super) fn evaluate(
        self,
        proxy_a: &NativeDistanceProxy,
        sweep_a: NativeSweep,
        proxy_b: &NativeDistanceProxy,
        sweep_b: NativeSweep,
        indices: (usize, usize),
        alpha: f32,
    ) -> f32 {
        let (index_a, index_b) = indices;
        let transform_a = sweep_a.transform(alpha);
        let transform_b = sweep_b.transform(alpha);
        match self {
            Self::Points { axis } => {
                let point_a = transform_a.point(proxy_a.vertices[index_a]);
                let point_b = transform_b.point(proxy_b.vertices[index_b]);
                native_toi_dot(native_toi_sub(point_b, point_a), axis)
            }
            Self::FaceA { axis, local_point } => {
                let normal = transform_a.rotate(axis);
                let point_a = transform_a.point(local_point);
                let point_b = transform_b.point(proxy_b.vertices[index_b]);
                native_toi_dot(native_toi_sub(point_b, point_a), normal)
            }
            Self::FaceB { axis, local_point } => {
                let normal = transform_b.rotate(axis);
                let point_b = transform_b.point(local_point);
                let point_a = transform_a.point(proxy_a.vertices[index_a]);
                native_toi_dot(native_toi_sub(point_a, point_b), normal)
            }
        }
    }
}

fn normalized_axis(vector: (f32, f32)) -> Option<(f32, f32)> {
    let length = native_toi_dot(vector, vector).sqrt();
    if length >= NATIVE_EPSILON {
        let inverse = length.recip();
        Some((vector.0 * inverse, vector.1 * inverse))
    } else {
        None
    }
}

fn normalize_point_axis(vector: (f32, f32)) -> (f32, f32) {
    // The point branch explicitly clears the output vector on the small-axis
    // path at 0x100862270.
    normalized_axis(vector).unwrap_or((0.0_f32, 0.0_f32))
}

fn normalize_face_axis(vector: (f32, f32)) -> (f32, f32) {
    // Both face branches store their perpendicular edge first. b2Vec2's
    // in-place Normalize then leaves that raw vector untouched below epsilon.
    normalized_axis(vector).unwrap_or(vector)
}

#[cfg(test)]
mod tests {
    use super::{normalize_face_axis, normalize_point_axis};

    #[test]
    fn initialize_axis_uses_native_float_epsilon_threshold() {
        let tiny = f32::EPSILON * 0.5_f32;
        assert_eq!(normalize_point_axis((tiny, 0.0)), (0.0, 0.0));
        assert_eq!(normalize_face_axis((tiny, 0.0)), (tiny, 0.0));
        assert_eq!(normalize_point_axis((f32::EPSILON, 0.0)), (1.0, 0.0));
    }
}
