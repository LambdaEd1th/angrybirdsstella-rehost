//! Per-frame Box2D joint-anchor export at `0x10005F944..0x10005FA98`.

use crate::*;

#[derive(Debug, Clone)]
pub(crate) struct NativeJointEndpointExport {
    pub(crate) name: String,
    pub(crate) joint_type: i32,
    pub(crate) first: (f32, f32),
    pub(crate) second: (f32, f32),
}

impl RenderBridge {
    pub(crate) fn native_joint_endpoint_exports(&self) -> Vec<NativeJointEndpointExport> {
        // GameLua+0x3c0/+0x3c8 delimit a 48-byte insertion-order vector. A
        // metadata-only type-five descriptor has no b2Joint pointer and is
        // therefore absent from this native walk.
        let mut joints = self
            .joints
            .values()
            .filter(|joint| {
                joint.is_physical && !self.joint_pending_native_destruction(&joint.name)
            })
            .collect::<Vec<_>>();
        joints.sort_by_key(|joint| joint.physics_creation_order);

        joints
            .into_iter()
            .filter_map(|joint| {
                let first = self.scene.get(&joint.first)?;
                let second = self.scene.get(&joint.second)?;
                Some(NativeJointEndpointExport {
                    name: joint.name.clone(),
                    joint_type: joint.joint_type,
                    // The two virtual calls return b2Vec2 values in S0/S1.
                    // Keep the body transform and Lua widening at float32.
                    first: first.native_transform_body_point((
                        joint.first_anchor.0 as f32,
                        joint.first_anchor.1 as f32,
                    )),
                    second: second.native_transform_body_point((
                        joint.second_anchor.0 as f32,
                        joint.second_anchor.1 as f32,
                    )),
                })
            })
            .collect()
    }
}
