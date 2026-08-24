//! Native nine-piece placement and vertical visibility gate.

use crate::*;

use super::{arguments::BoxDrawArguments, resource};

const FRAMEBUFFER_HEIGHT: f32 = 768.0;

pub(super) fn commands(
    arguments: &BoxDrawArguments,
    resources: &ResourceRuntime,
    data_root: &Path,
) -> Option<Vec<RenderCommand>> {
    let sprites = &arguments.sprites;
    let size = |sprite: Option<&str>| {
        [
            resource::width(sprite, resources) * arguments.scale_x,
            resource::height(sprite, resources) * arguments.scale_y,
        ]
    };
    let top_middle = size(sprites.top_middle.as_deref());
    let bottom_middle = size(sprites.bottom_middle.as_deref());
    let left = size(sprites.left.as_deref());
    let right = size(sprites.right.as_deref());
    let top_left = size(sprites.top_left.as_deref());
    let top_right = size(sprites.top_right.as_deref());
    let bottom_left = size(sprites.bottom_left.as_deref());
    let bottom_right = size(sprites.bottom_right.as_deref());
    let anchored_x = arguments.x + arguments.anchor_x;
    let anchored_y = arguments.y + arguments.anchor_y;
    let far_x = arguments.x + arguments.width - 1.0_f32 + arguments.anchor_x;
    let far_y = arguments.y + arguments.height - 1.0_f32 + arguments.anchor_y;

    if (arguments.y - top_middle[1] + arguments.anchor_y).floor() > FRAMEBUFFER_HEIGHT
        || (arguments.y + arguments.height - 1.0_f32 + arguments.anchor_y + bottom_middle[1])
            .floor()
            < 0.0
    {
        return None;
    }

    let mut commands = Vec::with_capacity(9);
    let mut emit = |sprite, rect: [f32; 4], horizontal_anchor, vertical_anchor| {
        commands.extend(resource::command(
            sprite,
            rect.map(f32::floor),
            horizontal_anchor,
            vertical_anchor,
            resources,
            data_root,
        ));
    };
    // Exact `sub_100052A98..sub_100053450` submission order.
    emit(
        sprites.top_middle.as_deref(),
        [
            anchored_x,
            arguments.y - top_middle[1] + arguments.anchor_y,
            arguments.width,
            top_middle[1],
        ],
        0,
        0,
    );
    emit(
        sprites.bottom_middle.as_deref(),
        [
            anchored_x,
            far_y + 1.0_f32,
            arguments.width,
            bottom_middle[1],
        ],
        0,
        0,
    );
    emit(
        sprites.left.as_deref(),
        [anchored_x - left[0], anchored_y, left[0], arguments.height],
        0,
        0,
    );
    emit(
        sprites.right.as_deref(),
        [far_x + 1.0_f32, anchored_y, right[0], arguments.height],
        0,
        0,
    );
    emit(
        sprites.top_left.as_deref(),
        [anchored_x, anchored_y, top_left[0], top_left[1]],
        resource::H_RIGHT,
        resource::V_BOTTOM,
    );
    emit(
        sprites.top_right.as_deref(),
        [far_x, anchored_y, top_right[0], top_right[1]],
        0,
        resource::V_BOTTOM,
    );
    emit(
        sprites.bottom_left.as_deref(),
        [anchored_x, far_y, bottom_left[0], bottom_left[1]],
        resource::H_RIGHT,
        0,
    );
    emit(
        sprites.bottom_right.as_deref(),
        [far_x, far_y, bottom_right[0], bottom_right[1]],
        0,
        0,
    );
    if arguments.background.is_none() {
        emit(
            sprites.center.as_deref(),
            [anchored_x, anchored_y, arguments.width, arguments.height],
            0,
            0,
        );
    }
    Some(commands)
}
