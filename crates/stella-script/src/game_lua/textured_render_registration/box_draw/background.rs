//! Color-table branch at `sub_100053244`.

use crate::*;

use super::arguments::BoxDrawArguments;

pub(super) fn submit(arguments: &BoxDrawArguments, bridge: &mut RenderBridge) {
    let Some(color) = arguments.background else {
        return;
    };
    bridge.state = RenderState::default();
    let left = (arguments.x + arguments.anchor_x).floor();
    let top = (arguments.y + arguments.anchor_y).floor();
    let right = (arguments.x + arguments.width - 1.0_f32 + arguments.anchor_x).floor();
    let bottom = (arguments.y + arguments.height - 1.0_f32 + arguments.anchor_y).floor();
    bridge.push_rect_command(native_rect_command(
        color.map(f64::from),
        f64::from(left),
        f64::from(top),
        f64::from(right),
        f64::from(bottom),
        RenderState::default(),
    ));
}
