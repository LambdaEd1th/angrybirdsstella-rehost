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
    let motor_speed = match table.raw_get::<Value>("motorSpeed")? {
        Value::Integer(value) => Some(value as f64),
        Value::Number(value) => Some(value),
        _ => None,
    };
    let max_torque = match table.raw_get::<Value>("maxTorque")? {
        Value::Integer(value) => value as f64,
        Value::Number(value) => value,
        // dword_1009AE7AC backs revolute torque and prismatic motor force.
        _ => 10_000.0,
    };
    let lower_limit = match table.raw_get::<Value>("lowerLimit")? {
        Value::Integer(value) => value as f64,
        Value::Number(value) => value,
        _ => 0.0,
    };
    let upper_limit = match table.raw_get::<Value>("upperLimit")? {
        Value::Integer(value) => value as f64,
        Value::Number(value) => value,
        _ if native_prismatic => 5.0,
        _ => 0.0,
    };
    Ok(JointParameters {
        collide_connected: table.get::<bool>("collideConnected").unwrap_or(false),
        destroy_timer: table.get::<f64>("destroyTimer").unwrap_or(0.0),
        breakable: table.get::<bool>("breakable").unwrap_or(false),
        break_force: table.get::<f64>("breakForce").unwrap_or(0.0),
        motor_enabled,
        motor_speed,
        max_torque,
        limits_enabled,
        lower_limit,
        upper_limit,
        frequency: table.get::<f64>("frequency").unwrap_or(0.0),
        damping_ratio: table.get::<f64>("dampingRatio").unwrap_or(0.0),
    })
}
