//! `createJointLua` phases split at the Lua/Box2D bridge boundary.

mod lua_bridge;
mod physics;

pub(crate) use lua_bridge::{dispatch_custom_joint, mirror_lua_joint_descriptor};
pub(crate) use physics::insert_physics_joint;
