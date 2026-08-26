//! Shared target-application bridge used by the native control members.

use std::path::Path;

use crate::*;

pub(super) fn apply_targets(
    runtime: &mut AnimationRuntime,
    resources: Option<&ResourceRuntime>,
    data_root: Option<&Path>,
    tag: &str,
    mode: u8,
) {
    if let (Some(resources), Some(data_root)) = (resources, data_root) {
        super::super::update::apply_native_targets_with_resources(
            runtime, resources, data_root, tag, mode,
        );
    } else {
        super::super::update::apply_native_targets(runtime, tag, mode);
    }
}

pub(super) fn advance_scene(
    runtime: &mut AnimationRuntime,
    resources: Option<&ResourceRuntime>,
    data_root: Option<&Path>,
    tag: &str,
    delta_time: f64,
) {
    if let (Some(resources), Some(data_root)) = (resources, data_root) {
        super::super::update::advance_native_scene_with_resources(
            runtime, resources, data_root, tag, delta_time,
        );
    } else {
        super::super::update::advance_native_scene(runtime, tag, delta_time);
    }
}
