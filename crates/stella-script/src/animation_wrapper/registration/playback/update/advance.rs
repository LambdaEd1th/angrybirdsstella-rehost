//! Control-time advancement and completion (sub_100411230).

use std::path::Path;

use super::apply::apply_native_targets_with_context;
use crate::*;

fn native_repeat_mode(mode: &str) -> bool {
    mode.is_empty() || mode == "repeat"
}

pub(super) fn advance_native_once(control: &AnimationControl, delta_time: f64) -> (f64, bool) {
    // Animation::Update at sub_100411230 performs all control arithmetic in
    // float32. State 3 is deliberately asymmetric: ordinary playback only
    // completes at the upper duration boundary. A control already exactly at
    // its duration (including a zero-duration static action) remains active
    // and is not completed again.
    let current = control.elapsed as f32;
    let duration = control.duration as f32;
    let delta = (delta_time as f32) * (control.speed as f32);
    let remaining = duration - current;
    if remaining > 0.0 {
        if delta >= remaining {
            (f64::from(duration), true)
        } else {
            (f64::from(current + delta), false)
        }
    } else if remaining < 0.0 {
        if delta >= -remaining {
            (f64::from(duration), true)
        } else {
            (f64::from(current - delta), false)
        }
    } else {
        (f64::from(current), false)
    }
}

fn retire_finished_control(
    runtime: &mut AnimationRuntime,
    resources: Option<&ResourceRuntime>,
    data_root: Option<&Path>,
    tag: &str,
    index: usize,
) {
    let Some(playback) = runtime.playback.get_mut(tag) else {
        return;
    };
    if index >= playback.controls.len() {
        return;
    }
    let mut control = playback.controls.swap_remove(index);
    control.elapsed = 0.0;
    control.previous_elapsed = 0.0;
    control.playing = false;
    control.paused = false;
    control.finished_pending_removal = false;
    if control.action == playback.current_action {
        playback.detached_current = Some(control);
    }
    // sub_100410EA0 seeks the removed control, which forces the remaining
    // target states to apply at their current times.
    apply_native_targets_with_context(runtime, resources, data_root, tag, 2);
}

/// Advance every active control in native vector order. Completion callbacks
/// read the wrapper component's shared mode, even when another action is the
/// control that reached its endpoint.
pub(in crate::animation_wrapper::registration::playback) fn advance_native_scene(
    runtime: &mut AnimationRuntime,
    tag: &str,
    delta_time: f64,
) {
    advance_native_scene_with_context(runtime, None, None, tag, delta_time);
}

pub(in crate::animation_wrapper::registration::playback) fn advance_native_scene_with_resources(
    runtime: &mut AnimationRuntime,
    resources: &ResourceRuntime,
    data_root: &Path,
    tag: &str,
    delta_time: f64,
) {
    advance_native_scene_with_context(runtime, Some(resources), Some(data_root), tag, delta_time);
}

fn advance_native_scene_with_context(
    runtime: &mut AnimationRuntime,
    resources: Option<&ResourceRuntime>,
    data_root: Option<&Path>,
    tag: &str,
    delta_time: f64,
) {
    let mut index = 0;
    loop {
        let Some(control_count) = runtime
            .playback
            .get(tag)
            .map(|playback| playback.controls.len())
        else {
            return;
        };
        if index >= control_count {
            break;
        }
        let remove = runtime.playback[tag].controls[index].finished_pending_removal;
        if remove {
            retire_finished_control(runtime, resources, data_root, tag, index);
            continue;
        }
        let completion = {
            let control = &mut runtime
                .playback
                .get_mut(tag)
                .expect("animation playback disappeared")
                .controls[index];
            if !control.playing || control.paused || control.speed == 0.0 {
                None
            } else {
                let (end_time, completed) = advance_native_once(control, delta_time);
                control.elapsed = end_time;
                completed.then_some(control.callback_installed)
            }
        };
        if let Some(callback_installed) = completion {
            if callback_installed {
                let mode = runtime.playback[tag].mode.clone();
                let event_name = if native_repeat_mode(&mode) {
                    runtime
                        .playback
                        .get_mut(tag)
                        .expect("animation playback disappeared")
                        .controls[index]
                        .elapsed = 0.0;
                    // sub_100016C50 seeks first, so a selected time-zero
                    // spineEvent is queued before PLAYBACK_REPEAT.
                    apply_native_targets_with_context(runtime, resources, data_root, tag, 2);
                    "PLAYBACK_REPEAT"
                } else if mode == "once" {
                    "PLAYBACK_END"
                } else {
                    ""
                };
                queue_animation_event(
                    runtime,
                    tag,
                    AnimationTimelineEvent {
                        name: event_name.to_owned(),
                        integer: 0,
                        number: 0.0,
                        text: String::new(),
                    },
                );
            } else {
                let control = &mut runtime
                    .playback
                    .get_mut(tag)
                    .expect("animation playback disappeared")
                    .controls[index];
                control.playing = false;
                control.finished_pending_removal = true;
            }
        }
        index += 1;
    }
    apply_native_targets_with_context(runtime, resources, data_root, tag, 3);
}
