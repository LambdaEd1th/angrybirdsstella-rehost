//! `GameLua::setLevelLimits` member at `sub_10004FFB8`.

use crate::*;

const PHYSICS_TO_FRAMEBUFFER_SCALE: f32 = 0.05_f32;

pub(super) fn apply(
    args: &MultiValue,
    environment: &mlua::Table,
    render: &Arc<Mutex<RenderBridge>>,
) -> LuaResult<()> {
    // sub_100088BC0 validates and narrows every argument before entering the
    // member. Its public order is x-min, y-min, x-max, y-max.
    let supplied = [
        native_required_number(args, 0, "setLevelLimits")? as f32,
        native_required_number(args, 1, "setLevelLimits")? as f32,
        native_required_number(args, 2, "setLevelLimits")? as f32,
        native_required_number(args, 3, "setLevelLimits")? as f32,
    ];
    let stored = [
        native_fcvtzs_f32(supplied[0]),
        native_fcvtzs_f32(supplied[2]),
        native_fcvtzs_f32(supplied[1]),
        native_fcvtzs_f32(supplied[3]),
    ];

    // The Particles notification deliberately reads the camera module's
    // accumulated global edges, not the four public collision limits. The
    // fallbacks keep isolated native-binding tests useful before camera.lua
    // has installed those globals; shipped game flow always supplies them.
    let current = [
        edge(environment, "levelLeftEdgePhysics", supplied[0])?,
        edge(environment, "levelRightEdgePhysics", supplied[2])?,
        edge(environment, "levelTopEdgePhysics", supplied[1])?,
        edge(environment, "levelBottomEdgePhysics", supplied[3])?,
    ];
    let old = [
        edge(environment, "oldLevelLeftEdgePhysics", current[0])?,
        edge(environment, "oldLevelRightEdgePhysics", current[1])?,
        edge(environment, "oldLevelTopEdgePhysics", current[2])?,
        edge(environment, "oldLevelBottomEdgePhysics", current[3])?,
    ];

    let mut bridge = render.lock().expect("render bridge lock poisoned");
    let current = drawable_particle_bounds(current, bridge.screen_width, bridge.screen_height);
    let old = drawable_particle_bounds(old, bridge.screen_width, bridge.screen_height);

    // Particles vtable slot +0x28 (`sub_100091544`) rescales only infinite
    // records around the old viewport centre before GameLua publishes the new
    // integer wrap bounds at +0x618..+0x624.
    bridge
        .particle_system
        .remap_infinite_level_limits(current, old);
    bridge.particle_wrap_limits =
        current.map(|bound| native_fcvtzs_f32(bound / PHYSICS_TO_FRAMEBUFFER_SCALE));
    bridge.level_limits = stored.map(f64::from);
    Ok(())
}

fn edge(environment: &mlua::Table, name: &str, fallback: f32) -> LuaResult<f32> {
    let value = environment.raw_get::<Value>(name)?;
    Ok(native_lua51_number(&value)
        .map(|number| number as f32)
        .unwrap_or(fallback))
}

fn drawable_particle_bounds(bounds: [f32; 4], width: u32, height: u32) -> [f32; 4] {
    let [left, right, top, bottom] = bounds.map(native_fcvtzs_f32);
    let aspect = (width as i32) as f32 / (height as i32) as f32;
    let vertical_span = bottom.wrapping_sub(top) as f32;
    // 0x10005031C is FNMSUB, so retain the single float32 rounding here.
    let vertical_adjustment = native_fcvtzs_f32((-aspect).mul_add(vertical_span, vertical_span));
    let half_adjustment = f64::from(vertical_adjustment) * 0.5_f64;
    let adjusted_top = native_fcvtzs_f64(f64::from(top) - half_adjustment);
    let adjusted_bottom = native_fcvtzs_f64(f64::from(bottom) + half_adjustment);
    [left, right, adjusted_top, adjusted_bottom].map(|value| value as f32)
}

fn native_fcvtzs_f64(value: f64) -> i32 {
    // 0x100050348/354/368/374 use FCVTZS W,D. Like the single-precision
    // variant, it saturates signed overflow and converts NaN to zero; it
    // does not have x86's universal integer-indefinite result.
    value as i32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adjusted_particle_bounds_saturate_the_native_double_to_word_conversion() {
        for (value, expected) in [
            (f64::NAN, 0),
            (f64::INFINITY, i32::MAX),
            (f64::NEG_INFINITY, i32::MIN),
            (2_147_483_647.75, i32::MAX),
            (2_147_483_648.0, i32::MAX),
            (-2_147_483_648.75, i32::MIN),
            (-2_147_483_649.0, i32::MIN),
            (42.75, 42),
            (-42.75, -42),
        ] {
            assert_eq!(native_fcvtzs_f64(value), expected, "{value:?}");
        }

        // This branch genuinely exceeds a signed word after the double
        // half-adjustment, even though the preceding bounds were int32.
        // The previous integer-indefinite approximation wrapped the positive
        // bottom edge to a large negative value instead of saturating.
        let bounds = drawable_particle_bounds([0.0, 100.0, 0.0, 2_147_483_520.0], 1, 2);
        assert_eq!(bounds, [0.0, 100.0, -536_870_880.0, i32::MAX as f32]);
    }

    #[cfg(target_arch = "aarch64")]
    #[test]
    fn particle_double_conversion_matches_actual_arm64_fcvtzs_word() {
        let mut bits = 0x5346_17A9_723B_045Du64;
        for fixed in [
            0,
            0x7FF0_0000_0000_0000,
            0xFFF0_0000_0000_0000,
            0x7FF8_0000_0000_0000,
            0x41E0_0000_0000_0000,
            0xC1E0_0000_0000_0000,
        ] {
            for index in 0..1024 {
                bits = bits
                    .wrapping_mul(6_364_136_223_846_793_005)
                    .wrapping_add(1_442_695_040_888_963_407);
                let value = f64::from_bits(if index == 0 { fixed } else { bits });
                let native: i32;
                // SAFETY: the instruction only accesses scalar registers;
                // every AArch64 target supports this conversion.
                unsafe {
                    std::arch::asm!(
                        "fcvtzs {result:w}, {value:d}",
                        result = out(reg) native,
                        value = in(vreg) value,
                        options(nomem, nostack),
                    );
                }
                assert_eq!(
                    native_fcvtzs_f64(value),
                    native,
                    "bits={:016x}",
                    value.to_bits()
                );
            }
        }
    }
}
