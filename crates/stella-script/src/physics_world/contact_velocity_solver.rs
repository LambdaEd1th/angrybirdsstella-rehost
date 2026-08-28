//! Box2D contact velocity-constraint initialization, warm start, and solve.

mod impulses;
mod initialization;
mod model;
mod solve;
mod storage;
mod warm_start;

pub(crate) use model::{
    ContactBodyState, ContactVelocityBodies, NativeContactVelocityCache,
    NativeContactVelocityConstraint, NativeContactVelocityPoint,
};

/// Purple evaluates `cross(radius, impulse)` as one rounded `FMUL` for
/// `radius.x * impulse.y`, followed by an `FNMSUB` for the second product.
fn native_contact_cross(radius: (f32, f32), impulse: (f32, f32)) -> f32 {
    let first_product = radius.0 * impulse.1;
    (-radius.1).mul_add(impulse.0, first_product)
}

/// Apply one signed contact impulse directly to the compact native velocity
/// triple. The original solver uses three FMADDs rather than pre-rounding a
/// delta and adding it afterward.
fn native_contact_velocity_write(
    velocity: &mut (f32, f32, f32),
    signed_inverse_mass: f32,
    signed_inverse_inertia: f32,
    impulse: (f32, f32),
    cross: f32,
) {
    velocity.0 = signed_inverse_mass.mul_add(impulse.0, velocity.0);
    velocity.1 = signed_inverse_mass.mul_add(impulse.1, velocity.1);
    velocity.2 = signed_inverse_inertia.mul_add(cross, velocity.2);
}

/// The two-point block solver adds its scalar impulse deltas before
/// multiplying by the normal for the shared linear velocity write.
fn native_contact_block_linear_impulse(normal: (f32, f32), deltas: [f32; 2]) -> (f32, f32) {
    let sum = deltas[0] + deltas[1];
    (normal.0 * sum, normal.1 * sum)
}

#[cfg(test)]
mod native_float_tests {
    use super::*;

    #[test]
    fn contact_cross_rounds_the_first_product_before_fused_subtraction() {
        let radius = (f32::from_bits(0x4229_6D75), f32::from_bits(0xC286_F8A3));
        let impulse = (f32::from_bits(0xC2C1_8DD3), f32::from_bits(0x4261_5B7E));
        let old_grouping = radius.0.mul_add(impulse.1, -(radius.1 * impulse.0));
        let native = native_contact_cross(radius, impulse);
        assert_eq!(old_grouping.to_bits(), 0xC581_8591);
        assert_eq!(native.to_bits(), 0xC581_8592);
    }

    #[test]
    fn contact_velocity_write_fuses_coefficient_into_old_velocity() {
        let mut velocity = (f32::from_bits(0x4294_B080), 0.0, 0.0);
        let coefficient = f32::from_bits(0xC1DE_EA72);
        let impulse = f32::from_bits(0x411F_A461);
        let separated = velocity.0 + coefficient * impulse;
        native_contact_velocity_write(&mut velocity, coefficient, 0.0, (impulse, 0.0), 0.0);
        assert_eq!(separated.to_bits(), 0xC34B_AD3E);
        assert_eq!(velocity.0.to_bits(), 0xC34B_AD3F);
    }

    #[test]
    fn contact_block_adds_deltas_before_normal_multiply() {
        let normal = (f32::from_bits(0x4226_65E3), 0.0);
        let deltas = [f32::from_bits(0x418B_31A7), f32::from_bits(0x4232_01B4)];
        let separated = normal.0 * deltas[0] + normal.0 * deltas[1];
        let native = native_contact_block_linear_impulse(normal, deltas);
        assert_eq!(separated.to_bits(), 0x4520_F0BF);
        assert_eq!(native.0.to_bits(), 0x4520_F0C0);
    }
}
