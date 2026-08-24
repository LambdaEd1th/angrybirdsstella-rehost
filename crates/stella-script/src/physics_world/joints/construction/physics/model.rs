//! Decoded native joint-definition records shared by switch phases.

pub(super) struct JointGeometry {
    pub(super) name: String,
    pub(super) first: String,
    pub(super) second: String,
    pub(super) joint_type: i32,
    pub(super) is_physical: bool,
    pub(super) first_anchor: (f64, f64),
    pub(super) second_anchor: (f64, f64),
    pub(super) local_axis: (f64, f64),
    pub(super) rest_angle: f64,
    pub(super) rest_length: f64,
    pub(super) one_way_destroy: bool,
}

pub(super) struct JointParameters {
    pub(super) collide_connected: bool,
    pub(super) destroy_timer: f64,
    pub(super) breakable: bool,
    pub(super) break_force: f64,
    pub(super) motor_enabled: bool,
    pub(super) motor_speed: Option<f64>,
    pub(super) max_torque: f64,
    pub(super) limits_enabled: bool,
    pub(super) lower_limit: f64,
    pub(super) upper_limit: f64,
    pub(super) frequency: f64,
    pub(super) damping_ratio: f64,
}
