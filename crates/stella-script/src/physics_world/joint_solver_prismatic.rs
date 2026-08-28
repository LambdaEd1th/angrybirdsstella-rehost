//! Prismatic-joint members recovered from its Box2D vtable.

mod impulses;
mod initialization;
mod position;
mod velocity;

use crate::{PhysicsJoint, PrismaticGeometry};

fn cached_prismatic_geometry(joint: &PhysicsJoint) -> PrismaticGeometry {
    PrismaticGeometry {
        delta: (0.0, 0.0),
        axis: joint.prismatic_axis,
        perpendicular: joint.prismatic_perpendicular,
        s1: joint.prismatic_s1,
        s2: joint.prismatic_s2,
        a1: joint.prismatic_a1,
        a2: joint.prismatic_a2,
    }
}

fn native_prismatic_mass_matrix(
    mass_first: f32,
    mass_second: f32,
    inertia_first: f32,
    inertia_second: f32,
    geometry: PrismaticGeometry,
) -> ((f64, f64, f64, f64, f64, f64), f64) {
    let (s1, s2, a1, a2) = (
        geometry.s1 as f32,
        geometry.s2 as f32,
        geometry.a1 as f32,
        geometry.a2 as f32,
    );
    let mass_sum = mass_first + mass_second;
    let k11 = s2.mul_add(
        inertia_second * s2,
        s1.mul_add(inertia_first * s1, mass_sum),
    );
    let k12 = (inertia_first * s1) + (inertia_second * s2);
    let k13 = (inertia_first * s1).mul_add(a1, (inertia_second * s2) * a2);
    let mut k22 = inertia_first + inertia_second;
    if k22 == 0.0 {
        k22 = 1.0;
    }
    let k23 = (inertia_first * a1) + (inertia_second * a2);
    let k33 = a2.mul_add(
        inertia_second * a2,
        a1.mul_add(inertia_first * a1, mass_sum),
    );
    let motor_mass = if k33 > 0.0 { k33.recip() } else { k33 };
    (
        (
            f64::from(k11),
            f64::from(k12),
            f64::from(k13),
            f64::from(k22),
            f64::from(k23),
            f64::from(k33),
        ),
        f64::from(motor_mass),
    )
}

fn native_solve_2x2(a11: f32, a12: f32, a22: f32, b1: f32, b2: f32) -> (f32, f32) {
    let solved = crate::solve_symmetric_2x2(
        f64::from(a11),
        f64::from(a12),
        f64::from(a22),
        f64::from(b1),
        f64::from(b2),
    )
    .unwrap_or((0.0, 0.0));
    (solved.0 as f32, solved.1 as f32)
}

fn native_solve_3x3(
    matrix: (f32, f32, f32, f32, f32, f32),
    rhs: (f32, f32, f32),
) -> (f32, f32, f32) {
    let solved = crate::solve_symmetric_3x3(
        (
            f64::from(matrix.0),
            f64::from(matrix.1),
            f64::from(matrix.2),
            f64::from(matrix.3),
            f64::from(matrix.4),
            f64::from(matrix.5),
        ),
        (f64::from(rhs.0), f64::from(rhs.1), f64::from(rhs.2)),
    )
    .unwrap_or((0.0, 0.0, 0.0));
    (solved.0 as f32, solved.1 as f32, solved.2 as f32)
}
