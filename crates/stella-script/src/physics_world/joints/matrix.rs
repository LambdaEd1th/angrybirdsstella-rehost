//! Box2D b2Mat22/b2Mat33 joint effective-mass helpers.

/// Symmetric joint effective-mass matrix in packed order
/// (k11, k12, k13, k22, k23, k33).
pub(crate) fn joint_mass_matrix(
    mass_a: f64,
    mass_b: f64,
    inertia_a: f64,
    inertia_b: f64,
    r_a: (f64, f64),
    r_b: (f64, f64),
) -> (f64, f64, f64, f64, f64, f64) {
    let mass_a = mass_a as f32;
    let mass_b = mass_b as f32;
    let inertia_a = inertia_a as f32;
    let inertia_b = inertia_b as f32;
    let r_a = (r_a.0 as f32, r_a.1 as f32);
    let r_b = (r_b.0 as f32, r_b.1 as f32);
    let mass_sum = mass_a + mass_b;
    let r_a_y_squared = r_a.1 * r_a.1;
    let mut k11 = inertia_a.mul_add(r_a_y_squared, mass_sum);
    let r_b_y_squared = r_b.1 * r_b.1;
    k11 = inertia_b.mul_add(r_b_y_squared, k11);
    let k12 = (-inertia_a * r_a.0).mul_add(r_a.1, (-inertia_b * r_b.0) * r_b.1);
    let k13 = (-inertia_a).mul_add(r_a.1, -inertia_b * r_b.1);
    let r_a_x_squared = r_a.0 * r_a.0;
    let mut k22 = inertia_a.mul_add(r_a_x_squared, mass_sum);
    let r_b_x_squared = r_b.0 * r_b.0;
    k22 = inertia_b.mul_add(r_b_x_squared, k22);
    let k23 = inertia_a.mul_add(r_a.0, inertia_b * r_b.0);
    (
        f64::from(k11),
        f64::from(k12),
        f64::from(k13),
        f64::from(k22),
        f64::from(k23),
        f64::from(inertia_a + inertia_b),
    )
}

pub(crate) fn solve_symmetric_2x2(
    a11: f64,
    a12: f64,
    a22: f64,
    b1: f64,
    b2: f64,
) -> Option<(f64, f64)> {
    let (a11, a12, a22, b1, b2) = (a11 as f32, a12 as f32, a22 as f32, b1 as f32, b2 as f32);
    // sub_100862E14 forms the negated determinant. A singular matrix leaves
    // its reciprocal at zero and therefore returns a zero vector.
    let determinant = (-a11).mul_add(a22, a12 * a12);
    let inverse = if determinant != 0.0_f32 {
        determinant.recip()
    } else {
        0.0_f32
    };
    Some((
        f64::from(inverse * (-a22).mul_add(b1, a12 * b2)),
        f64::from(inverse * (-a11).mul_add(b2, a12 * b1)),
    ))
}

pub(crate) fn solve_symmetric_3x3(
    matrix: (f64, f64, f64, f64, f64, f64),
    rhs: (f64, f64, f64),
) -> Option<(f64, f64, f64)> {
    let (a11, a12, a13, a22, a23, a33) = (
        matrix.0 as f32,
        matrix.1 as f32,
        matrix.2 as f32,
        matrix.3 as f32,
        matrix.4 as f32,
        matrix.5 as f32,
    );
    let (b1, b2, b3) = (rhs.0 as f32, rhs.1 as f32, rhs.2 as f32);
    // Exact sub_100862D60 cofactor order. Like Solve22, the native helper
    // returns zero for a singular matrix rather than requesting a fallback.
    let cofactor_x = (-a22).mul_add(a33, a23 * a23);
    let cofactor_y = (-a23).mul_add(a13, a33 * a12);
    let cofactor_z = (-a23).mul_add(a12, a22 * a13);
    let determinant = a13.mul_add(cofactor_z, a11.mul_add(cofactor_x, a12 * cofactor_y));
    let inverse = if determinant != 0.0_f32 {
        determinant.recip()
    } else {
        0.0_f32
    };
    let x = inverse * b3.mul_add(cofactor_z, b1.mul_add(cofactor_x, b2 * cofactor_y));

    let y_cofactor_x = (-a33).mul_add(b2, a23 * b3);
    let y_cofactor_y = (-a13).mul_add(b3, a33 * b1);
    let y_cofactor_z = (-a23).mul_add(b1, a13 * b2);
    let y = inverse * a13.mul_add(y_cofactor_z, a11.mul_add(y_cofactor_x, a12 * y_cofactor_y));

    let z_cofactor_x = (-a22).mul_add(b3, a23 * b2);
    let z_cofactor_y = (-a23).mul_add(b1, a12 * b3);
    let z_cofactor_z = (-a12).mul_add(b2, a22 * b1);
    let z = inverse * a13.mul_add(z_cofactor_z, a11.mul_add(z_cofactor_x, a12 * z_cofactor_y));
    Some((f64::from(x), f64::from(y), f64::from(z)))
}
