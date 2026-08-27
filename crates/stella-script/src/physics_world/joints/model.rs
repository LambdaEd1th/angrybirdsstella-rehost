//! Persistent joint state corresponding to Purple's Box2D joint subclasses.

#[derive(Debug, Clone)]
pub(crate) struct PhysicsJoint {
    pub(crate) physics_creation_order: u64,
    pub(crate) name: String,
    pub(crate) first: String,
    pub(crate) second: String,
    pub(crate) joint_type: i32,
    pub(crate) coord_type: i32,
    /// Whether the descriptor owns a Box2D constraint. Native type 5 is
    /// always a metadata-only destruction link; only type 4 is prismatic.
    pub(crate) is_physical: bool,
    pub(crate) first_anchor: (f64, f64),
    pub(crate) second_anchor: (f64, f64),
    /// Prismatic axis in end1's local frame, matching
    /// b2PrismaticJointDef::Initialize.
    pub(crate) local_axis: (f64, f64),
    pub(crate) rest_angle: f64,
    pub(crate) rest_length: f64,
    pub(crate) collide_connected: bool,
    pub(crate) destroy_timer: f64,
    pub(crate) one_way_destroy: bool,
    pub(crate) breakable: bool,
    pub(crate) break_force: f64,
    pub(crate) motor_enabled: bool,
    pub(crate) motor_speed: Option<f64>,
    pub(crate) max_torque: f64,
    pub(crate) linear_impulse_x: f64,
    pub(crate) linear_impulse_y: f64,
    pub(crate) angular_impulse: f64,
    pub(crate) motor_impulse: f64,
    pub(crate) limit_impulse: f64,
    pub(crate) distance_impulse: f64,
    pub(crate) previous_step: f64,
    pub(crate) limits_enabled: bool,
    pub(crate) lower_limit: f64,
    pub(crate) upper_limit: f64,
    pub(crate) limit_state: JointLimitState,
    pub(crate) frequency: f64,
    pub(crate) damping_ratio: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum JointLimitState {
    Inactive,
    AtLower,
    AtUpper,
    Equal,
}
