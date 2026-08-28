//! `b2TimeOfImpact` (`sub_100861B54`) conservative advancement.

use super::super::{
    NativeDistanceProxy, NativeSweep, distance::native_core_distance, simplex::NativeSimplexCache,
};
use super::function::NativeSeparationFunction;

/// Twenty outer iterations, eight separation-axis pushes and alternating
/// bisection/secant roots capped at fifty evaluations.
pub(crate) fn native_time_of_impact(
    proxy_a: &NativeDistanceProxy,
    mut sweep_a: NativeSweep,
    proxy_b: &NativeDistanceProxy,
    mut sweep_b: NativeSweep,
) -> Option<f32> {
    const LINEAR_SLOP: f32 = 0.001;
    const MAX_ITERATIONS: usize = 20;
    const MAX_PUSH_BACK_ITERATIONS: usize = 8;
    const MAX_ROOT_ITERATIONS: usize = 50;

    sweep_a.normalize();
    sweep_b.normalize();
    let target = LINEAR_SLOP.max(proxy_a.radius + proxy_b.radius - 3.0_f32 * LINEAR_SLOP);
    let tolerance = 0.25_f32 * LINEAR_SLOP;
    let lower_target = target - tolerance;
    let upper_target = target + tolerance;
    let mut alpha_1 = 0.0_f32;
    // Purple clears b2SimplexCache::count once before entering the TOI outer
    // loop. b2Distance then reads and rewrites the same metric/feature cache
    // on every conservative-advancement iteration.
    let mut cache = NativeSimplexCache::default();

    for _ in 0..MAX_ITERATIONS {
        let distance = native_core_distance(
            proxy_a,
            sweep_a.transform(alpha_1),
            proxy_b,
            sweep_b.transform(alpha_1),
            &mut cache,
        );
        if distance <= 0.0_f32 {
            return Some(0.0_f32);
        }
        if distance < upper_target {
            return Some(alpha_1);
        }

        let separation = NativeSeparationFunction::initialize(
            cache, proxy_a, sweep_a, proxy_b, sweep_b, alpha_1,
        );
        let mut alpha_2 = 1.0_f32;
        let mut advance_outer = false;
        for _ in 0..MAX_PUSH_BACK_ITERATIONS {
            let (separation_2, index_a, index_b) =
                separation.find_min_separation(proxy_a, sweep_a, proxy_b, sweep_b, alpha_2);
            if separation_2 > upper_target {
                return None;
            }
            if separation_2 > lower_target {
                alpha_1 = alpha_2;
                advance_outer = true;
                break;
            }
            let separation_1 = separation.evaluate(
                proxy_a,
                sweep_a,
                proxy_b,
                sweep_b,
                (index_a, index_b),
                alpha_1,
            );
            if separation_1 < lower_target {
                return None;
            }
            if separation_1 <= upper_target {
                return Some(alpha_1);
            }

            let mut root_lower = alpha_1;
            let mut root_upper = alpha_2;
            let mut value_lower = separation_1;
            let mut value_upper = separation_2;
            for root_iteration in 0..MAX_ROOT_ITERATIONS {
                let alpha = if root_iteration & 1 == 0 {
                    (root_lower + root_upper) * 0.5_f32
                } else {
                    root_lower
                        + (target - value_lower) * (root_upper - root_lower)
                            / (value_upper - value_lower)
                };
                let value = separation.evaluate(
                    proxy_a,
                    sweep_a,
                    proxy_b,
                    sweep_b,
                    (index_a, index_b),
                    alpha,
                );
                if (value - target).abs() < tolerance {
                    alpha_2 = alpha;
                    break;
                }
                if value > target {
                    root_lower = alpha;
                    value_lower = value;
                } else {
                    root_upper = alpha;
                    value_upper = value;
                }
            }
        }
        if !advance_outer {
            return None;
        }
    }
    None
}
