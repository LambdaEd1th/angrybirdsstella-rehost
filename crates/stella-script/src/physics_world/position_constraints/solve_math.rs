//! Shared float32 kernel from Purple's ordinary and TOI position solvers.

/// Contact body A's angular position write forms `-cross(radius, impulse)`
/// with a rounded `radius.x * impulse.y`, then `FNMSUB` for the positive
/// `radius.y * impulse.x` term.
pub(crate) fn native_contact_position_negative_cross(
    radius: (f32, f32),
    impulse: (f32, f32),
) -> f32 {
    radius.1.mul_add(impulse.0, -(radius.0 * impulse.1))
}

/// Contact body B and both effective-mass levers round
/// `radius.y * impulse.x` before fusing the positive product.
pub(crate) fn native_contact_position_positive_cross(
    radius: (f32, f32),
    impulse: (f32, f32),
) -> f32 {
    radius.0.mul_add(impulse.1, -(radius.1 * impulse.0))
}

pub(crate) fn native_position_effective_inverse_mass(
    first_inverse_mass: f32,
    second_inverse_mass: f32,
    first_inverse_inertia: f32,
    second_inverse_inertia: f32,
    first_lever: f32,
    second_lever: f32,
) -> f32 {
    // 0x100864878..888 and 0x100864BAC..BBC square each lever with FMUL,
    // then add the two inertia terms with consecutive FMADD instructions.
    let mut inverse_mass = first_inverse_mass + second_inverse_mass;
    inverse_mass = first_inverse_inertia.mul_add(first_lever * first_lever, inverse_mass);
    second_inverse_inertia.mul_add(second_lever * second_lever, inverse_mass)
}

pub(crate) fn native_position_correction(
    separation: f32,
    baumgarte: f32,
    effective_inverse_mass: f32,
) -> f32 {
    // FCMP/B.LE also takes this branch for an unordered effective mass.
    if effective_inverse_mass.partial_cmp(&0.0_f32) != Some(std::cmp::Ordering::Greater) {
        return 0.0_f32;
    }
    let scaled_error = (separation + 0.001_f32) * baumgarte;
    let non_positive_error = scaled_error.min(0.0_f32);
    let magnitude = if non_positive_error < -0.2_f32 {
        0.2_f32
    } else {
        -non_positive_error
    };
    magnitude / effective_inverse_mass
}

#[cfg(test)]
mod tests {
    use super::{
        native_contact_position_negative_cross, native_contact_position_positive_cross,
        native_position_correction, native_position_effective_inverse_mass,
    };

    #[test]
    fn contact_position_crosses_keep_each_native_write_grouping() {
        let radius = (f32::from_bits(0x4229_6D75), f32::from_bits(0xC286_F8A3));
        let impulse = (f32::from_bits(0xC2C1_8DD3), f32::from_bits(0x4261_5B7E));

        // The older shared helper fuses the negative product. Contact body B
        // instead rounds that product and fuses the positive one.
        let old_shared_grouping = (-radius.1).mul_add(impulse.0, radius.0 * impulse.1);
        assert_eq!(old_shared_grouping.to_bits(), 0xC581_8592);
        assert_eq!(
            native_contact_position_positive_cross(radius, impulse).to_bits(),
            0xC581_8591
        );
        assert_eq!(
            native_contact_position_negative_cross(radius, impulse).to_bits(),
            0x4581_8592
        );
    }

    #[test]
    fn unordered_effective_mass_skips_the_native_correction() {
        assert_eq!(native_position_correction(-1.0, 0.2, f32::NAN).to_bits(), 0);
    }

    #[test]
    fn effective_mass_squares_levers_before_the_two_native_fmadds() {
        let mass_a = f32::from_bits(0x3F1A_B105);
        let mass_b = f32::from_bits(0x3F4E_EEA0);
        let inertia_a = f32::from_bits(0x3F24_D17F);
        let inertia_b = f32::from_bits(0x3F2D_30D2);
        let lever_a = f32::from_bits(0x3F30_1A09);
        let lever_b = f32::from_bits(0xBF20_DBD4);

        let native = native_position_effective_inverse_mass(
            mass_a, mass_b, inertia_a, inertia_b, lever_a, lever_b,
        );
        assert_eq!(native.to_bits(), 0x3FFD_FF97);

        let multiply_inertia_first =
            (inertia_a * lever_a).mul_add(lever_a, mass_a + mass_b) + inertia_b * lever_b * lever_b;
        assert_eq!(multiply_inertia_first.to_bits(), 0x3FFD_FF98);
    }
}
