//! Per-joint parameter/default decoding after native class selection.

use mlua::{Result as LuaResult, Value};

use super::model::{JointGeometry, JointParameters};

pub(super) fn decode_joint_parameters(
    table: &mlua::Table,
    geometry: &JointGeometry,
) -> LuaResult<JointParameters> {
    let native_prismatic = matches!(geometry.joint_type, 4 | 5) && geometry.is_physical;
    let motor_enabled = match table.raw_get::<Value>("motor")? {
        Value::Boolean(value) => value,
        _ => native_prismatic,
    };
    let limits_enabled = match table.raw_get::<Value>("limit")? {
        Value::Boolean(value) => value,
        _ => native_prismatic,
    };
    let motor_speed = native_optional_number(table, "motorSpeed")?;
    let max_torque = match native_optional_number(table, "maxTorque")? {
        Some(value) => value,
        // dword_1009AE7AC backs revolute torque and prismatic motor force.
        None => f64::from(10_000.0_f32),
    };
    let lower_limit = match native_optional_number(table, "lowerLimit")? {
        Some(value) => value,
        None => f64::from(0.0_f32),
    };
    let upper_limit = match native_optional_number(table, "upperLimit")? {
        Some(value) => value,
        None if native_prismatic => f64::from(5.0_f32),
        // b2RevoluteJointDef::upperAngle is initialized from 0x1009F74B0.
        None if geometry.joint_type == 3 => f64::from(std::f32::consts::PI),
        None => f64::from(0.0_f32),
    };
    let collide_connected = table.get::<bool>("collideConnected").unwrap_or(false);
    let frequency = native_optional_number(table, "frequency")?.unwrap_or_else(|| {
        f64::from(if geometry.joint_type == 1 {
            4.0_f32
        } else {
            0.0_f32
        })
    });
    let damping_ratio = native_optional_number(table, "dampingRatio")?.unwrap_or_else(|| {
        f64::from(if geometry.joint_type == 1 {
            0.5_f32
        } else {
            0.0_f32
        })
    });

    if geometry.joint_type == 1 {
        // The native distance-joint branch publishes the resolved definition
        // fields back to the descriptor immediately after CreateJoint.
        table.set("frequency", frequency)?;
        table.set("dampingRatio", damping_ratio)?;
        table.set("collideConnected", collide_connected)?;
    }

    Ok(JointParameters {
        collide_connected,
        destroy_timer: native_optional_number(table, "destroyTimer")?.unwrap_or(0.0),
        breakable: table.get::<bool>("breakable").unwrap_or(false),
        break_force: native_optional_number(table, "breakForce")?.unwrap_or(0.0),
        motor_enabled,
        motor_speed,
        max_torque,
        limits_enabled,
        lower_limit,
        upper_limit,
        frequency,
        damping_ratio,
    })
}

fn native_optional_number(table: &mlua::Table, field: &str) -> LuaResult<Option<f64>> {
    Ok(match table.raw_get::<Value>(field)? {
        Value::Integer(value) => Some(f64::from(value as f32)),
        Value::Number(value) => Some(f64::from(value as f32)),
        _ => None,
    })
}
