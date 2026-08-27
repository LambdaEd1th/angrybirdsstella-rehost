//! `createJoint` coordinate-mode switch and per-class anchor initialization.

use mlua::{Result as LuaResult, Value};

use crate::{RenderBridge, native_hypot};

use super::super::super::geometry::inverse_rotate_vector;
use super::model::JointGeometry;

pub(super) fn decode_joint_geometry(
    bridge: &RenderBridge,
    table: &mlua::Table,
) -> LuaResult<Option<JointGeometry>> {
    let name = table.get::<String>("name").unwrap_or_default();
    let first_name = table.get::<String>("end1").unwrap_or_default();
    let second_name = table.get::<String>("end2").unwrap_or_default();
    if name.is_empty() || first_name.is_empty() || second_name.is_empty() {
        return Ok(None);
    }
    let Some(first) = bridge.scene.get(&first_name) else {
        return Ok(None);
    };
    let Some(second) = bridge.scene.get(&second_name) else {
        return Ok(None);
    };
    let raw_first_anchor = (
        table.get::<f64>("x1").unwrap_or(0.0),
        table.get::<f64>("y1").unwrap_or(0.0),
    );
    let raw_second_anchor = (
        table.get::<f64>("x2").unwrap_or(0.0),
        table.get::<f64>("y2").unwrap_or(0.0),
    );
    let coord_type = table
        .get::<f32>("coordType")
        .map(|value| (value + 0.5_f32).floor() as i32)
        .unwrap_or(0);
    let (common_first_anchor, common_second_anchor) = match coord_type {
        // sub_1000386EC uses both body centers when coordType is absent/zero.
        0 => ((0.0, 0.0), (0.0, 0.0)),
        // sub_100038474 subtracts translation but deliberately does not
        // inverse-rotate before the joint definition later rotates it.
        1 => (
            (raw_first_anchor.0 - first.x, raw_first_anchor.1 - first.y),
            (
                raw_second_anchor.0 - second.x,
                raw_second_anchor.1 - second.y,
            ),
        ),
        // All 3,803 shipped physical descriptors already use body-local data.
        2 => (raw_first_anchor, raw_second_anchor),
        _ => ((0.0, 0.0), (0.0, 0.0)),
    };
    let joint_type = table.get::<i32>("type").unwrap_or(0);
    if joint_type >= 7 {
        return Ok(None);
    }
    let one_way_destroy_value = table.raw_get::<Value>("oneWayDestroy")?;
    // Type 5 is always the metadata-only destroy-link record. The optional
    // boolean affects direction, not the native joint class.
    let is_physical = matches!(joint_type, 1..=4 | 6);
    let one_way_destroy = matches!(&one_way_destroy_value, Value::Boolean(true));

    let (descriptor_first_anchor, descriptor_second_anchor) = if joint_type == 6 {
        match coord_type {
            0 => ((-1.0, 0.0), (1.0, 0.0)),
            1 | 2 => (common_first_anchor, common_second_anchor),
            _ => ((-1.0, 0.0), (1.0, 0.0)),
        }
    } else {
        (common_first_anchor, common_second_anchor)
    };
    let descriptor_first_world = first.native_transform_body_point((
        descriptor_first_anchor.0 as f32,
        descriptor_first_anchor.1 as f32,
    ));
    let descriptor_first_world = (
        f64::from(descriptor_first_world.0),
        f64::from(descriptor_first_world.1),
    );
    let descriptor_second_world = second.native_transform_body_point((
        descriptor_second_anchor.0 as f32,
        descriptor_second_anchor.1 as f32,
    ));
    let descriptor_second_world = (
        f64::from(descriptor_second_world.0),
        f64::from(descriptor_second_world.1),
    );
    let (first_anchor, second_anchor) = match joint_type {
        // Weld averages both supplied world anchors before Initialize.
        2 => {
            let midpoint = (
                f64::from(
                    (descriptor_first_world.0 as f32 + descriptor_second_world.0 as f32) * 0.5_f32,
                ),
                f64::from(
                    (descriptor_first_world.1 as f32 + descriptor_second_world.1 as f32) * 0.5_f32,
                ),
            );
            (
                first.native_inverse_transform_body_point(midpoint),
                second.native_inverse_transform_body_point(midpoint),
            )
        }
        // Revolute and prismatic initialize from end1's common world point.
        3 => (
            descriptor_first_anchor,
            second.native_inverse_transform_body_point(descriptor_first_world),
        ),
        4 | 5 if is_physical => (
            descriptor_first_anchor,
            second.native_inverse_transform_body_point(descriptor_first_world),
        ),
        _ => (descriptor_first_anchor, descriptor_second_anchor),
    };
    let first_world_anchor =
        first.native_transform_body_point((first_anchor.0 as f32, first_anchor.1 as f32));
    let second_world_anchor =
        second.native_transform_body_point((second_anchor.0 as f32, second_anchor.1 as f32));
    let native_anchor_length = native_hypot(
        second_world_anchor.0 - first_world_anchor.0,
        second_world_anchor.1 - first_world_anchor.1,
    );
    let rest_length = if joint_type == 6 {
        let inferred = match coord_type {
            0 => native_hypot((second.x - first.x) as f32, (second.y - first.y) as f32),
            1 | 2 => native_anchor_length,
            _ => 0.0,
        };
        f64::from(table.get::<f32>("maxLength").unwrap_or(inferred))
    } else {
        f64::from(table.get::<f32>("length").unwrap_or(native_anchor_length))
    };
    if joint_type == 1 {
        // createJoint publishes b2DistanceJoint+0xA4 immediately to Lua.
        table.set("length", rest_length)?;
    }
    let local_axis = if matches!(joint_type, 4 | 5) && is_physical {
        inverse_rotate_vector(
            (
                table.get::<f64>("worldAxisX").unwrap_or(0.0),
                table.get::<f64>("worldAxisY").unwrap_or(0.0),
            ),
            first.angle,
        )
    } else {
        (0.0, 0.0)
    };

    Ok(Some(JointGeometry {
        name,
        first: first_name,
        second: second_name,
        joint_type,
        coord_type,
        is_physical,
        first_anchor,
        second_anchor,
        local_axis,
        rest_angle: second.angle - first.angle,
        rest_length,
        one_way_destroy,
    }))
}
