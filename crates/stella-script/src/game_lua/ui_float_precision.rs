//! Float32 Lua-number boundaries used by the recovered UI timing path.

use super::StellaLua;
use crate::*;

impl StellaLua {
    /// Purple's bundled Lua 5.1 VM stores `lua_Number` in four bytes. mlua
    /// uses doubles, so restore the float32 arithmetic that is observable in
    /// the startup LEAVES transition's Frame delays, black-cover Tween and
    /// three accumulating animation scales, as well as every easing curve
    /// shipped by `scripts_common/tween.lua`.
    pub(super) fn install_native_ui_float_precision(&self) -> Result<(), ScriptError> {
        let environment = game_environment(&self.lua)?;
        let prepare_subtraction =
            self.lua
                .create_function(|_, (time_left, delta): (f64, f64)| {
                    Ok(prepare_float32_subtraction(time_left, delta))
                })?;
        let prepare_addition = self.lua.create_function(|_, (timer, delta): (f64, f64)| {
            Ok(prepare_float32_addition(timer, delta))
        })?;
        let advance_scales = self.lua.create_function(
            |_, (first, second, third, delta): (f64, f64, f64, f64)| {
                let [first, second, third] = native_startup_leaf_scales(
                    [first as f32, second as f32, third as f32],
                    delta as f32,
                );
                Ok((f64::from(first), f64::from(second), f64::from(third)))
            },
        )?;
        environment.set("__stellaPrepareFrameDelaySubtract", prepare_subtraction)?;
        environment.set("__stellaPrepareTweenTimerAdd", prepare_addition)?;
        environment.set("__stellaAdvanceStartupLeafScales", advance_scales)?;
        self.execute_source(
            r##"
            do
                local frameClass = ui and ui.Frame
                local prepareSubtract = __stellaPrepareFrameDelaySubtract
                local prepareTweenAdd = __stellaPrepareTweenTimerAdd
                local advanceLeafScales = __stellaAdvanceStartupLeafScales
                __stellaPrepareFrameDelaySubtract = nil
                __stellaPrepareTweenTimerAdd = nil
                __stellaAdvanceStartupLeafScales = nil
                if frameClass and not rawget(frameClass, "__stellaNativeDelayPrecision") then
                    local shippedUpdate = frameClass.update
                    frameClass.update = function(self, scaledDelta, rawDelta)
                        local delta = gamelua.g_realDt or scaledDelta
                        if delta ~= nil and self.delayedCalls ~= nil then
                            for _, delayedCall in ipairs(self.delayedCalls) do
                                delayedCall.timeLeft = prepareSubtract(delayedCall.timeLeft, delta)
                            end
                        end
                        return shippedUpdate(self, scaledDelta, rawDelta)
                    end
                    frameClass.__stellaNativeDelayPrecision = true
                end
                if frameClass and not rawget(frameClass, "__stellaNativeLeafScalePrecision") then
                    local shippedAddChild = frameClass.addChild
                    frameClass.addChild = function(self, child, ...)
                        if child.animScale ~= nil and child.animScale2 ~= nil and
                           child.animScale3 ~= nil and not child.__stellaNativeLeafScales then
                            local shippedChildUpdate = child.update
                            child.update = function(leafFrame, scaledDelta, rawDelta)
                                local first = leafFrame.animScale
                                local second = leafFrame.animScale2
                                local third = leafFrame.animScale3
                                shippedChildUpdate(leafFrame, scaledDelta, rawDelta)
                                leafFrame.animScale, leafFrame.animScale2, leafFrame.animScale3 =
                                    advanceLeafScales(first, second, third, scaledDelta)
                            end
                            child.__stellaNativeLeafScales = true
                        end
                        return shippedAddChild(self, child, ...)
                    end
                    frameClass.__stellaNativeLeafScalePrecision = true
                end
                if TweenSubsystem and
                   not rawget(TweenSubsystem, "__stellaNativeTimerPrecision") then
                    local shippedAddTween = TweenSubsystem.add
                    TweenSubsystem.add = function(self, parameters)
                        local tween = shippedAddTween(self, parameters)
                        local shippedTweenUpdate = tween.update
                        tween.update = function(activeTween, delta)
                            activeTween.timer = prepareTweenAdd(activeTween.timer, delta)
                            return shippedTweenUpdate(activeTween, delta)
                        end
                        return tween
                    end
                    TweenSubsystem.__stellaNativeTimerPrecision = true
                end
            end
            "##,
        )?;
        self.install_native_tween_curves(&environment)
    }

    fn install_native_tween_curves(&self, environment: &mlua::Table) -> Result<(), ScriptError> {
        for (name, curve) in NATIVE_TWEEN_CURVES {
            let function = self.lua.create_function(
                move |_, (time, start, change, duration): (Value, Value, Value, Value)| {
                    let number = |value: &Value, argument: usize| {
                        native_lua51_number(value).ok_or_else(|| {
                            runtime_error(format!("{name} argument {argument} must be a number"))
                        })
                    };
                    Ok(f64::from(curve.evaluate(
                        number(&time, 1)? as f32,
                        number(&start, 2)? as f32,
                        number(&change, 3)? as f32,
                        number(&duration, 4)? as f32,
                    )))
                },
            )?;
            environment.set(name, function)?;
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug)]
enum NativeTweenCurve {
    Linear,
    CubicIn,
    CubicOut,
    CubicInOut,
    QuadraticIn,
    QuadraticOut,
    BounceOut,
    SineIn,
    SineOut,
    SineInOut,
}

const NATIVE_TWEEN_CURVES: [(&str, NativeTweenCurve); 10] = [
    ("tweenLinear", NativeTweenCurve::Linear),
    ("tweenEaseCubicIn", NativeTweenCurve::CubicIn),
    ("tweenEaseCubicOut", NativeTweenCurve::CubicOut),
    ("tweenEaseCubicInOut", NativeTweenCurve::CubicInOut),
    ("tweenEaseQuadraticIn", NativeTweenCurve::QuadraticIn),
    ("tweenEaseQuadraticOut", NativeTweenCurve::QuadraticOut),
    ("tweenEaseBounceOut", NativeTweenCurve::BounceOut),
    ("tweenEaseSineIn", NativeTweenCurve::SineIn),
    ("tweenEaseSineOut", NativeTweenCurve::SineOut),
    ("tweenEaseSineInOut", NativeTweenCurve::SineInOut),
];

impl NativeTweenCurve {
    fn evaluate(self, time: f32, start: f32, change: f32, duration: f32) -> f32 {
        match self {
            Self::Linear => native_linear(time, start, change, duration),
            Self::CubicIn => native_cubic_in(time, start, change, duration),
            Self::CubicOut => native_cubic_out(time, start, change, duration),
            Self::CubicInOut => native_cubic_in_out(time, start, change, duration),
            Self::QuadraticIn => native_quadratic_in(time, start, change, duration),
            Self::QuadraticOut => native_quadratic_out(time, start, change, duration),
            Self::BounceOut => native_bounce_out(time, start, change, duration),
            Self::SineIn => native_sine_in(time, start, change, duration),
            Self::SineOut => native_sine_out(time, start, change, duration),
            Self::SineInOut => native_sine_in_out(time, start, change, duration),
        }
    }
}

fn prepare_float32_subtraction(value: f64, delta: f64) -> f64 {
    let native_result = f64::from((value as f32) - (delta as f32));
    native_result + delta
}

fn prepare_float32_addition(value: f64, delta: f64) -> f64 {
    let native_result = f64::from((value as f32) + (delta as f32));
    native_result - delta
}

fn native_linear(time: f32, start: f32, change: f32, duration: f32) -> f32 {
    let mut result = change * time;
    result /= duration;
    result + start
}

fn native_cubic_in(time: f32, start: f32, change: f32, duration: f32) -> f32 {
    let normalized = time / duration;
    let mut result = change * normalized;
    result *= normalized;
    result *= normalized;
    result + start
}

fn native_cubic_out(time: f32, start: f32, change: f32, duration: f32) -> f32 {
    let normalized = (time / duration) - 1.0_f32;
    let mut result = normalized * normalized;
    result *= normalized;
    result += 1.0_f32;
    result *= change;
    result + start
}

fn native_cubic_in_out(time: f32, start: f32, change: f32, duration: f32) -> f32 {
    let half_duration = duration / 2.0_f32;
    let mut normalized = time / half_duration;
    if normalized < 1.0_f32 {
        let half_change = change / 2.0_f32;
        let square = normalized * normalized;
        let cube = square * normalized;
        return (half_change * cube) + start;
    }
    normalized -= 2.0_f32;
    let half_change = change / 2.0_f32;
    let square = normalized * normalized;
    let cube = square * normalized;
    let shifted = cube + 2.0_f32;
    (half_change * shifted) + start
}

fn native_quadratic_in(time: f32, start: f32, change: f32, duration: f32) -> f32 {
    let normalized = time / duration;
    let mut result = change * normalized;
    result *= normalized;
    result + start
}

fn native_quadratic_out(time: f32, start: f32, change: f32, duration: f32) -> f32 {
    let normalized = (time / duration) - 1.0_f32;
    let mut result = -normalized;
    result *= normalized;
    result += 1.0_f32;
    result *= change;
    result + start
}

fn native_bounce_out(time: f32, start: f32, change: f32, duration: f32) -> f32 {
    let mut normalized = time / duration;
    if normalized < 0.363_636_37_f32 {
        let mut result = change * 7.5625_f32;
        result *= normalized;
        result *= normalized;
        return result + start;
    }
    if normalized < 0.727_272_75_f32 {
        normalized -= 0.545_454_56_f32;
        let mut result = 7.5625_f32 * normalized;
        result *= normalized;
        result += 0.75_f32;
        result *= change;
        return result + start;
    }
    if normalized < 0.909_090_94_f32 {
        normalized -= 0.818_181_8_f32;
        let mut result = 7.5625_f32 * normalized;
        result *= normalized;
        result += 0.9375_f32;
        result *= change;
        return result + start;
    }
    normalized -= 0.954_545_44_f32;
    let mut result = 7.5625_f32 * normalized;
    result *= normalized;
    result += 0.984_375_f32;
    result *= change;
    result + start
}

fn native_lua_cos(value: f32) -> f32 {
    // Purple 0x100517D38: Lua float -> fcvt d0,s0 -> libm cos -> fcvt s0,d0.
    f64::from(value).cos() as f32
}

fn native_lua_sin(value: f32) -> f32 {
    // Purple 0x100518348: Lua float -> fcvt d0,s0 -> libm sin -> fcvt s0,d0.
    f64::from(value).sin() as f32
}

fn native_sine_in(time: f32, start: f32, change: f32, duration: f32) -> f32 {
    let mut angle = time / duration;
    angle *= std::f32::consts::PI;
    angle *= 0.5_f32;
    let trig = native_lua_cos(angle);
    let mut result = 1.0_f32 - trig;
    result *= change;
    start + result
}

fn native_sine_out(time: f32, start: f32, change: f32, duration: f32) -> f32 {
    let mut angle = time / duration;
    angle *= std::f32::consts::PI;
    angle *= 0.5_f32;
    let trig = native_lua_sin(angle);
    let result = trig * change;
    start + result
}

fn native_sine_in_out(time: f32, start: f32, change: f32, duration: f32) -> f32 {
    let mut angle = time / duration;
    angle *= std::f32::consts::PI;
    let trig = native_lua_cos(angle);
    let mut result = 1.0_f32 - trig;
    result *= 0.5_f32;
    result *= change;
    start + result
}

fn native_startup_leaf_scales(scales: [f32; 3], delta: f32) -> [f32; 3] {
    [
        scales[0] + (delta * 0.4_f32),
        scales[1] + (delta * 0.2_f32),
        scales[2] + (delta * 0.1_f32),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn startup_leaf_scalars_keep_float32_operation_boundaries() {
        let delta = 1.0_f32 / 60.0_f32;
        let mut scales = [1.0_f32; 3];
        for _ in 0..97 {
            scales = native_startup_leaf_scales(scales, delta);
        }
        assert_eq!(
            scales.map(f32::to_bits),
            [0x3fd2_c5f4, 0x3fa9_62fa, 0x3f94_b17d]
        );

        assert_eq!(
            native_cubic_in_out(0.4_f32, 1.0_f32, -1.0_f32, 1.0_f32).to_bits(),
            0x3f3e_76c8
        );
    }

    #[test]
    fn shipped_tween_curves_keep_every_float32_operation_boundary() {
        let common = (0.4_f32, 1.0_f32, -1.0_f32, 1.0_f32);
        let expected = [
            (NativeTweenCurve::Linear, 0x3f19_999a),
            (NativeTweenCurve::CubicIn, 0x3f6f_9db2),
            (NativeTweenCurve::CubicOut, 0x3e5d_2f1c),
            (NativeTweenCurve::CubicInOut, 0x3f3e_76c8),
            (NativeTweenCurve::QuadraticIn, 0x3f57_0a3d),
            (NativeTweenCurve::QuadraticOut, 0x3eb8_51ec),
            (NativeTweenCurve::SineIn, 0x3f4f_1bbd),
            (NativeTweenCurve::SineOut, 0x3ed3_0dd0),
            (NativeTweenCurve::SineInOut, 0x3f27_8dde),
        ];
        for (curve, bits) in expected {
            assert_eq!(
                curve
                    .evaluate(common.0, common.1, common.2, common.3)
                    .to_bits(),
                bits,
                "{curve:?}"
            );
        }

        for (time, bits) in [
            (0.2_f32, 0x3f32_8f5c),
            (0.5_f32, 0x3e70_0000),
            (0.8_f32, 0x3d75_c290),
            (0.95_f32, 0x3c7d_70c0),
        ] {
            assert_eq!(
                NativeTweenCurve::BounceOut
                    .evaluate(time, common.1, common.2, common.3)
                    .to_bits(),
                bits,
                "bounce branch at {time}"
            );
        }
    }
}
