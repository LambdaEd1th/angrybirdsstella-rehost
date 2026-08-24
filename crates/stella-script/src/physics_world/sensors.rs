//! Native sensor-force facade matching the recovered dispatcher/callee split.

mod force_dispatch;
mod water;

pub(crate) use force_dispatch::{
    apply_native_sensor_forces, apply_native_sensor_forces_to_object, native_hypot,
};
