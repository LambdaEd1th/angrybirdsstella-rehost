//! Persistent joint state corresponding to Purple's Box2D joint subclasses.

#[derive(Debug, Clone)]
pub(crate) struct PhysicsJoint {
    pub(crate) physics_creation_order: u64,
    pub(crate) name: String,
    pub(crate) first: String,
    pub(crate) second: String,
    pub(crate) joint_type: i32,
    pub(crate) coord_type: i32,
    /// Whether the descriptor selects a Box2D joint class. Native type 5 is
    /// always a metadata-only destruction link; only type 4 is prismatic.
    pub(crate) is_physical: bool,
    /// Whether `b2World::CreateJoint` returned a concrete native joint.
    /// During a contact callback the world is locked: types 2/3/4/6 still
    /// leave a GameLua `jointData` record, but its `b2Joint*` is null.
    pub(crate) native_joint_present: bool,
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
    /// Float solver cache populated by b2DistanceJoint::InitVelocityConstraints.
    pub(crate) distance_axis: (f64, f64),
    pub(crate) distance_radius_first: (f64, f64),
    pub(crate) distance_radius_second: (f64, f64),
    pub(crate) distance_inverse_mass_first: f64,
    pub(crate) distance_inverse_mass_second: f64,
    pub(crate) distance_inverse_inertia_first: f64,
    pub(crate) distance_inverse_inertia_second: f64,
    pub(crate) distance_current_length: f64,
    pub(crate) distance_effective_mass: f64,
    pub(crate) distance_gamma: f64,
    pub(crate) distance_bias: f64,
    pub(crate) weld_radius_first: (f64, f64),
    pub(crate) weld_radius_second: (f64, f64),
    pub(crate) weld_inverse_mass_first: f64,
    pub(crate) weld_inverse_mass_second: f64,
    pub(crate) weld_inverse_inertia_first: f64,
    pub(crate) weld_inverse_inertia_second: f64,
    /// Symmetric 3x3 matrix in `(k11, k12, k13, k22, k23, k33)` order.
    pub(crate) weld_mass_matrix: (f64, f64, f64, f64, f64, f64),
    /// Float solver cache populated by b2PrismaticJoint::InitVelocityConstraints.
    pub(crate) prismatic_axis: (f64, f64),
    pub(crate) prismatic_perpendicular: (f64, f64),
    pub(crate) prismatic_s1: f64,
    pub(crate) prismatic_s2: f64,
    pub(crate) prismatic_a1: f64,
    pub(crate) prismatic_a2: f64,
    pub(crate) prismatic_inverse_mass_first: f64,
    pub(crate) prismatic_inverse_mass_second: f64,
    pub(crate) prismatic_inverse_inertia_first: f64,
    pub(crate) prismatic_inverse_inertia_second: f64,
    /// Symmetric 3x3 matrix in `(k11, k12, k13, k22, k23, k33)` order.
    pub(crate) prismatic_mass_matrix: (f64, f64, f64, f64, f64, f64),
    pub(crate) prismatic_motor_mass: f64,
    pub(crate) revolute_radius_first: (f64, f64),
    pub(crate) revolute_radius_second: (f64, f64),
    pub(crate) revolute_inverse_mass_first: f64,
    pub(crate) revolute_inverse_mass_second: f64,
    pub(crate) revolute_inverse_inertia_first: f64,
    pub(crate) revolute_inverse_inertia_second: f64,
    /// Symmetric 3x3 matrix in `(k11, k12, k13, k22, k23, k33)` order.
    pub(crate) revolute_mass_matrix: (f64, f64, f64, f64, f64, f64),
    pub(crate) revolute_motor_mass: f64,
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

impl PhysicsJoint {
    pub(crate) fn has_native_joint(&self) -> bool {
        self.is_physical && self.native_joint_present
    }
}
