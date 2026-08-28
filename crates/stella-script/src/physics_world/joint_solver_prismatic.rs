//! Prismatic-joint members recovered from its Box2D vtable.

mod impulses;
mod initialization;
mod position;
mod velocity;

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
