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
    if !value.is_finite() || !(-2_147_483_648.0_f64..2_147_483_648.0_f64).contains(&value) {
        i32::MIN
    } else {
        value.trunc() as i32
    }
}
