//! Shared float32 kernel from Purple's ordinary and TOI position solvers.

pub(crate) fn native_position_cross(radius: (f32, f32), impulse: (f32, f32)) -> f32 {
    (-radius.1).mul_add(impulse.0, radius.0 * impulse.1)
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
    if effective_inverse_mass <= 0.0_f32 {
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
    use super::native_position_effective_inverse_mass;

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
