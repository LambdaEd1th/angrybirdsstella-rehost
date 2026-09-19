//! Pure-Rust integer clipping for `DirtMechanics::cut` (`sub_100020D70`).

mod clean;

use clipper2_rust::{
    ClipType, Clipper64, FillRule, Path64, Paths64, Point64, PolyTree64, poly_tree_to_paths64,
};

use crate::*;
use clean::clean_clipper_polygon;

pub(crate) fn native_dirt_input_integer(value: f64) -> i64 {
    // 0x100021010/101C and 0x1000210A0/10AC convert to W first, then
    // SXTW to Clipper's 64-bit coordinate. This is not FCVTZS X,S.
    i64::from(native_fcvtzs_f32((value as f32) * 1000.0))
}

fn native_dirt_slit_left(slit_right: i64) -> i64 {
    // 0x100020EE0 quantizes the centre to W. SUB W8,W8,#1 at
    // 0x100020F00 wraps before SXTW at 0x100020F04; INT_MIN therefore
    // becomes INT_MAX rather than clamping or subtracting in 64 bits.
    i64::from((slit_right as i32).wrapping_sub(1))
}

fn native_dirt_slit(slit_right: i64) -> Path64 {
    const SLIT_LIMIT: i64 = 100_000;
    let slit_left = native_dirt_slit_left(slit_right);
    vec![
        Point64::new(slit_right, -SLIT_LIMIT),
        Point64::new(slit_right, SLIT_LIMIT),
        Point64::new(slit_left, SLIT_LIMIT),
        Point64::new(slit_left, -SLIT_LIMIT),
    ]
}

pub(crate) fn native_dirt_output_coord(value: i64) -> f64 {
    // sub_100020D70 uses `scvtf s0, xN` followed by an `fmul` with its
    // 0.001f literal before storing the result in math::float2.
    f64::from((value as f32) * 0.001_f32)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct NativeClipperPoint {
    pub(crate) x: i64,
    pub(crate) y: i64,
}

pub(crate) fn native_dirt_octagon(hole: DirtHole) -> Vec<NativeClipperPoint> {
    #[allow(clippy::excessive_precision)]
    const NATIVE_ANGLE_STEP: f32 = 0.785000026_f32;
    let center_x = hole.local_x as f32;
    let center_y = hole.local_y as f32;
    let radius = hole.radius as f32;
    (0..8)
        .map(|index| {
            // Literal recovered at sub_100020D70+0xD0. Purple intentionally
            // uses 0.785000026f rather than an exact PI/4 constant. It
            // multiplies in double, then narrows the angle for sincosf.
            let angle = (f64::from(index) * f64::from(NATIVE_ANGLE_STEP)) as f32;
            NativeClipperPoint {
                x: native_dirt_input_integer(f64::from(radius.mul_add(angle.cos(), center_x))),
                y: native_dirt_input_integer(f64::from(radius.mul_add(angle.sin(), center_y))),
            }
        })
        .collect()
}

pub(crate) fn native_dirt_difference(
    paths: &[Vec<(f64, f64)>],
    hole: DirtHole,
) -> Vec<Vec<(f64, f64)>> {
    let cut = native_dirt_octagon(hole);
    let slit_right = native_dirt_input_integer(hole.local_x);
    let mut result = Vec::new();
    // sub_100020D70 executes one Clipper instance per existing foreground
    // DrawablePolygon rather than unioning every subject contour first.
    for subject in paths {
        let subject = subject
            .iter()
            .map(|&(x, y)| NativeClipperPoint {
                x: native_dirt_input_integer(x),
                y: native_dirt_input_integer(y),
            })
            .collect::<Vec<_>>();
        let clipped = clipper_dirt_difference(&subject, &cut, slit_right);
        result.extend(clipped.into_iter().filter_map(|path| {
            let path = path
                .into_iter()
                .map(|point| {
                    (
                        native_dirt_output_coord(point.x),
                        native_dirt_output_coord(point.y),
                    )
                })
                .collect::<Vec<_>>();
            let open_length = path.windows(2).fold(0.0_f32, |length, edge| {
                let delta_x = edge[1].0 as f32 - edge[0].0 as f32;
                let delta_y = edge[1].1 as f32 - edge[0].1 as f32;
                length + delta_x.mul_add(delta_x, delta_y * delta_y).sqrt()
            });
            (open_length >= 1.0_f32).then_some(path)
        }));
    }
    result
}

fn clipper_dirt_difference(
    subject: &[NativeClipperPoint],
    cut: &[NativeClipperPoint],
    slit_right: i64,
) -> Vec<Vec<NativeClipperPoint>> {
    let subject = to_clipper_path(subject);
    let cut = to_clipper_path(cut);
    if subject.len() < 3 || cut.len() < 3 {
        return Vec::new();
    }

    let subjects = vec![subject];
    let cuts = vec![cut];
    let mut clipper = Clipper64::new();
    clipper.add_subject(&subjects);
    clipper.add_clip(&cuts);
    let mut tree = PolyTree64::new();
    let mut open_paths = Paths64::new();
    if !clipper.execute_tree(
        ClipType::Difference,
        FillRule::NonZero,
        &mut tree,
        &mut open_paths,
    ) {
        return Vec::new();
    }
    let mut slit_applied = false;
    if (1..tree.nodes.len()).any(|node| tree.is_hole(node)) {
        // Purple adds this one-unit-wide rectangle to the existing Clipper
        // instance and executes the difference again. Clipper2's Rust port
        // cleans its scanline state after execute, so rebuild the equivalent
        // complete input set before the second execution.
        // The caller supplies the already sign-extended 32-bit grid point;
        // the opposite edge must retain the native W-register subtraction.
        let slit = native_dirt_slit(slit_right);
        let all_cuts = vec![cuts[0].clone(), slit];
        let mut slit_clipper = Clipper64::new();
        slit_clipper.add_subject(&subjects);
        slit_clipper.add_clip(&all_cuts);
        if !slit_clipper.execute_tree(
            ClipType::Difference,
            FillRule::NonZero,
            &mut tree,
            &mut open_paths,
        ) {
            return Vec::new();
        }
        slit_applied = true;
    }

    poly_tree_to_paths64(&tree)
        .into_iter()
        .map(|contour| {
            let mut path = contour
                .into_iter()
                .map(|point| NativeClipperPoint {
                    x: point.x,
                    y: point.y,
                })
                .collect::<Vec<_>>();
            if slit_applied {
                align_clipper_6_slit_contour(&mut path, slit_right);
            }
            clean_clipper_polygon(path, 20.0)
        })
        .collect()
}

fn align_clipper_6_slit_contour(path: &mut [NativeClipperPoint], slit_right: i64) {
    let slit_left = native_dirt_slit_left(slit_right);
    if path.is_empty() || path.iter().any(|point| point.x > slit_left) {
        return;
    }

    // Clipper 6.2.1's scanline output record leaves the lower end of the
    // left slit edge as OutRec::Pts. BuildResult2 starts at Pts->Prev, which
    // therefore makes that vertex the first item in the flattened contour.
    // Clipper2 preserves the same cycle but selects the following vertex.
    if let Some((index, _)) = path
        .iter()
        .enumerate()
        .filter(|(_, point)| point.x == slit_left)
        .min_by_key(|(_, point)| point.y)
    {
        path.rotate_left(index);
    }
}

fn to_clipper_path(points: &[NativeClipperPoint]) -> Path64 {
    points
        .iter()
        .map(|point| Point64::new(point.x, point.y))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dirt_clipper_grid_sign_extends_saturated_word_before_wrapping_slit_edge() {
        for (input, right, left) in [
            (f64::NAN, 0, -1),
            (f64::INFINITY, i64::from(i32::MAX), i64::from(i32::MAX - 1)),
            (f64::NEG_INFINITY, i64::from(i32::MIN), i64::from(i32::MAX)),
            (-1.25, -1250, -1251),
            (1.25, 1250, 1249),
        ] {
            let actual_right = native_dirt_input_integer(input);
            assert_eq!(actual_right, right, "{input:?}");
            assert_eq!(native_dirt_slit_left(actual_right), left, "{input:?}");
            let slit = native_dirt_slit(actual_right);
            assert_eq!(
                slit.iter()
                    .map(|point| (point.x, point.y))
                    .collect::<Vec<_>>(),
                vec![
                    (right, -100_000),
                    (right, 100_000),
                    (left, 100_000),
                    (left, -100_000)
                ]
            );
        }
    }

    #[cfg(target_arch = "aarch64")]
    #[test]
    fn dirt_clipper_grid_and_slit_match_actual_arm64_word_instruction_sequence() {
        for input in [
            f64::NAN,
            f64::INFINITY,
            f64::NEG_INFINITY,
            -1e20,
            1e20,
            -1.25,
            1.25,
        ] {
            let input = input as f32;
            let scaled: f32;
            let right: i64;
            let left: i64;
            // SAFETY: register-only reproduction of Purple's float32 grid
            // multiply, FCVTZS W,S, SUB W,#1 and SXTW instructions.
            unsafe {
                std::arch::asm!(
                    "fmul {scaled:s}, {value:s}, {factor:s}",
                    "fcvtzs {right:w}, {scaled:s}",
                    "sub {left:w}, {right:w}, #1",
                    "sxtw {right:x}, {right:w}",
                    "sxtw {left:x}, {left:w}",
                    scaled = out(vreg) scaled,
                    value = in(vreg) input,
                    factor = in(vreg) 1000.0f32,
                    right = out(reg) right,
                    left = out(reg) left,
                    options(nomem, nostack),
                );
            }
            let _ = scaled;
            assert_eq!(
                native_dirt_input_integer(f64::from(input)),
                right,
                "{input:?}"
            );
            assert_eq!(native_dirt_slit_left(right), left, "{input:?}");
        }
    }
}
