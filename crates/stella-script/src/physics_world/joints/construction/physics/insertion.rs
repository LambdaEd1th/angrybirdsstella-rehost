//! `b2World::CreateJoint` state insertion and deferred contact filtering.

use crate::{JointLimitState, PhysicsJoint, RenderBridge};

use super::model::{JointGeometry, JointParameters};

pub(super) fn insert_joint(
    bridge: &mut RenderBridge,
    geometry: JointGeometry,
    parameters: JointParameters,
) {
    let physics_creation_order = bridge.allocate_physics_creation_order();
    let filter_first = geometry.first.clone();
    let filter_second = geometry.second.clone();
    let is_physical = geometry.is_physical;
    let collide_connected = parameters.collide_connected;
    if let Some(previous) = bridge.joints.get(&geometry.name) {
        let previous_order = previous.physics_creation_order;
        bridge.remove_native_joint_order(previous_order);
    }
    if is_physical {
        bridge.insert_native_joint_order(
            physics_creation_order,
            geometry.name.clone(),
            &geometry.first,
            &geometry.second,
        );
    }
    bridge.joints.insert(
        geometry.name.clone(),
        PhysicsJoint {
            physics_creation_order,
            name: geometry.name,
            first: geometry.first,
            second: geometry.second,
            joint_type: geometry.joint_type,
            coord_type: geometry.coord_type,
            is_physical,
            first_anchor: geometry.first_anchor,
            second_anchor: geometry.second_anchor,
            local_axis: geometry.local_axis,
            rest_angle: geometry.rest_angle,
            rest_length: geometry.rest_length,
            collide_connected,
            destroy_timer: parameters.destroy_timer,
            one_way_destroy: geometry.one_way_destroy,
            breakable: parameters.breakable,
            break_force: parameters.break_force,
            motor_enabled: parameters.motor_enabled,
            motor_speed: parameters.motor_speed,
            max_torque: parameters.max_torque,
            linear_impulse_x: 0.0,
            linear_impulse_y: 0.0,
            angular_impulse: 0.0,
            motor_impulse: 0.0,
            limit_impulse: 0.0,
            distance_impulse: 0.0,
            distance_axis: (0.0, 0.0),
            distance_radius_first: (0.0, 0.0),
            distance_radius_second: (0.0, 0.0),
            distance_inverse_mass_first: 0.0,
            distance_inverse_mass_second: 0.0,
            distance_inverse_inertia_first: 0.0,
            distance_inverse_inertia_second: 0.0,
            distance_current_length: 0.0,
            distance_effective_mass: 0.0,
            distance_gamma: 0.0,
            distance_bias: 0.0,
            weld_radius_first: (0.0, 0.0),
            weld_radius_second: (0.0, 0.0),
            weld_inverse_mass_first: 0.0,
            weld_inverse_mass_second: 0.0,
            weld_inverse_inertia_first: 0.0,
            weld_inverse_inertia_second: 0.0,
            weld_mass_matrix: (0.0, 0.0, 0.0, 0.0, 0.0, 0.0),
            prismatic_axis: (0.0, 0.0),
            prismatic_perpendicular: (0.0, 0.0),
            prismatic_s1: 0.0,
            prismatic_s2: 0.0,
            prismatic_a1: 0.0,
            prismatic_a2: 0.0,
            prismatic_inverse_mass_first: 0.0,
            prismatic_inverse_mass_second: 0.0,
            prismatic_inverse_inertia_first: 0.0,
            prismatic_inverse_inertia_second: 0.0,
            prismatic_mass_matrix: (0.0, 0.0, 0.0, 0.0, 0.0, 0.0),
            prismatic_motor_mass: 0.0,
            revolute_radius_first: (0.0, 0.0),
            revolute_radius_second: (0.0, 0.0),
            revolute_inverse_mass_first: 0.0,
            revolute_inverse_mass_second: 0.0,
            revolute_inverse_inertia_first: 0.0,
            revolute_inverse_inertia_second: 0.0,
            revolute_mass_matrix: (0.0, 0.0, 0.0, 0.0, 0.0, 0.0),
            revolute_motor_mass: 0.0,
            previous_step: 0.0,
            limits_enabled: parameters.limits_enabled,
            lower_limit: parameters.lower_limit,
            upper_limit: parameters.upper_limit,
            limit_state: JointLimitState::Inactive,
            frequency: parameters.frequency,
            damping_ratio: parameters.damping_ratio,
        },
    );
    // `sub_10086E470` flags only existing contacts, without waking endpoints.
    if is_physical && !collide_connected {
        bridge.flag_contacts_for_filtering_between(&filter_first, &filter_second);
    }
}
