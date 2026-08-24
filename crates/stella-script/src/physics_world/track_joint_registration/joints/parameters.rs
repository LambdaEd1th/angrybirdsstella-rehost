//! `setJointParameters` (`sub_10003E890`) concrete-joint dispatch.

use crate::*;

pub(super) fn install(
    lua: &Lua,
    globals: &mlua::Table,
    render: Arc<Mutex<RenderBridge>>,
) -> LuaResult<()> {
    globals.set(
        "setJointParameters",
        lua.create_function(move |lua, table: mlua::Table| {
            let name = table.get::<String>("name").unwrap_or_default();
            let optional_bool = |field: &str| -> LuaResult<Option<bool>> {
                match table.raw_get::<Value>(field)? {
                    Value::Boolean(value) => Ok(Some(value)),
                    _ => Ok(None),
                }
            };
            let optional_number = |field: &str| -> LuaResult<Option<f64>> {
                let value = table.raw_get::<Value>(field)?;
                Ok(value_number(&value).map(|value| f64::from(value as f32)))
            };

            let mut bridge = render.lock().expect("render bridge lock poisoned");
            let Some(mut joint) = bridge.joints.get(&name).cloned() else {
                return Ok(());
            };
            // The native switch is on b2JointType: distance accepts its
            // spring/length triplet; revolute and prismatic accept motor/limit
            // fields; weld, rope and metadata joints accept none.
            let supports_motor = joint.is_physical && matches!(joint.joint_type, 3..=5);
            let supports_distance = joint.is_physical && joint.joint_type == 1;
            let motor = supports_motor
                .then(|| optional_bool("motor"))
                .transpose()?
                .flatten();
            let motor_speed = supports_motor
                .then(|| optional_number("motorSpeed"))
                .transpose()?
                .flatten();
            let max_torque = supports_motor
                .then(|| optional_number("maxTorque"))
                .transpose()?
                .flatten();
            let limit = supports_motor
                .then(|| optional_bool("limit"))
                .transpose()?
                .flatten();
            let lower_limit = supports_motor
                .then(|| optional_number("lowerLimit"))
                .transpose()?
                .flatten();
            let upper_limit = supports_motor
                .then(|| optional_number("upperLimit"))
                .transpose()?
                .flatten();
            let frequency = supports_distance
                .then(|| optional_number("frequency"))
                .transpose()?
                .flatten();
            let damping_ratio = supports_distance
                .then(|| optional_number("dampingRatio"))
                .transpose()?
                .flatten();
            let length = supports_distance
                .then(|| optional_number("length"))
                .transpose()?
                .flatten();

            let mut wake_bodies = false;
            if let Some(value) = motor {
                wake_bodies |= joint.motor_enabled != value;
                joint.motor_enabled = value;
            }
            if let Some(value) = motor_speed {
                wake_bodies |= joint.motor_speed != Some(value);
                joint.motor_speed = Some(value);
            }
            if let Some(value) = max_torque {
                wake_bodies |= joint.max_torque != value;
                joint.max_torque = value;
            }
            if let Some(value) = limit {
                if joint.limits_enabled != value {
                    joint.limit_impulse = 0.0;
                    joint.limit_state = JointLimitState::Inactive;
                    wake_bodies = true;
                }
                joint.limits_enabled = value;
            }
            if let Some(value) = lower_limit {
                if joint.lower_limit != value {
                    joint.limit_impulse = 0.0;
                    joint.limit_state = JointLimitState::Inactive;
                    wake_bodies = true;
                }
                joint.lower_limit = value;
            }
            if let Some(value) = upper_limit {
                if joint.upper_limit != value {
                    joint.limit_impulse = 0.0;
                    joint.limit_state = JointLimitState::Inactive;
                    wake_bodies = true;
                }
                joint.upper_limit = value;
            }
            if let Some(value) = frequency {
                joint.frequency = value;
            }
            if let Some(value) = damping_ratio {
                joint.damping_ratio = value;
            }
            if let Some(value) = length {
                joint.rest_length = value;
            }
            let endpoints = [joint.first.clone(), joint.second.clone()];
            bridge.joints.insert(name.clone(), joint);
            if wake_bodies {
                for endpoint in endpoints {
                    if let Some(object) = bridge.scene.get_mut(&endpoint) {
                        object.motion_started = true;
                        object.wake();
                    }
                }
            }
            drop(bridge);

            // Mirror only fields accepted by the concrete Box2D joint type.
            if let Some(descriptor) = joint_descriptor(lua, &name)? {
                for (field, value) in [
                    ("motorSpeed", motor_speed),
                    ("maxTorque", max_torque),
                    ("lowerLimit", lower_limit),
                    ("upperLimit", upper_limit),
                    ("frequency", frequency),
                    ("dampingRatio", damping_ratio),
                    ("length", length),
                ] {
                    if let Some(value) = value {
                        descriptor.set(field, value)?;
                    }
                }
                if let Some(value) = motor {
                    descriptor.set("motor", value)?;
                }
                if let Some(value) = limit {
                    descriptor.set("limit", value)?;
                }
            }
            Ok(())
        })?,
    )?;
    Ok(())
}

fn joint_descriptor(lua: &Lua, name: &str) -> LuaResult<Option<mlua::Table>> {
    let environment = game_environment(lua)?;
    let Value::Table(objects) = environment.get::<Value>("objects")? else {
        return Ok(None);
    };
    let Value::Table(joints) = objects.get::<Value>("joints")? else {
        return Ok(None);
    };
    if let Value::Table(descriptor) = joints.raw_get::<Value>(name)? {
        return Ok(Some(descriptor));
    }
    Ok(joints
        .pairs::<Value, Value>()
        .filter_map(Result::ok)
        .find_map(|(_, value)| match value {
            Value::Table(descriptor)
                if descriptor.get::<String>("name").ok().as_deref() == Some(name) =>
            {
                Some(descriptor)
            }
            _ => None,
        }))
}
