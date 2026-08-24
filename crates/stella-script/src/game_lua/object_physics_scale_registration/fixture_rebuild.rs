//! Native fixture-replacement facade recovered from `sub_10004050C`.

mod circle;
mod lifecycle;
mod polygon;

use super::arguments::FixtureCoefficients;
use crate::*;

pub(super) fn polygon(
    lua: &Lua,
    render: &Arc<Mutex<RenderBridge>>,
    name: &str,
    ratios: (f32, f32),
    coefficients: FixtureCoefficients,
    sensor: bool,
) -> LuaResult<()> {
    polygon::rebuild(lua, render, name, ratios, coefficients, sensor)
}

pub(super) fn circle(
    lua: &Lua,
    render: &Arc<Mutex<RenderBridge>>,
    name: &str,
    radius: f64,
    fixture_scale: f32,
    coefficients: FixtureCoefficients,
    sensor: bool,
) -> LuaResult<()> {
    circle::rebuild(
        lua,
        render,
        name,
        radius,
        fixture_scale,
        coefficients,
        sensor,
    )
}
