//! Foreground `offsetY = "top"/"bottom"` cache rebuild.
//!
//! `sub_100099828` projects a zero-offset layer through every corrected
//! camera. `sub_1000985DC` then selects an extreme and stores the resolved
//! float in native layer+0x40.

use crate::*;

#[derive(Debug, Clone, Copy)]
pub(super) struct ResolutionCamera {
    pub(super) scale: f32,
    #[allow(dead_code)]
    pub(super) x: f32,
    pub(super) y: f32,
    #[allow(dead_code)]
    pub(super) left: f32,
    pub(super) top: f32,
}

pub(super) fn resolve_symbolic_foreground_offsets(
    bridge: &mut RenderBridge,
    cameras: &[ResolutionCamera],
) {
    let end_scale = bridge.resolution_camera_scale;
    let reference_scale = bridge.theme_camera.scale;
    let current_scale = bridge.world_scale as f32;
    let reference_y = bridge.theme_camera.y;
    let orientation = bridge.theme_camera.orientation;
    let screen_height = bridge.screen_height as f32;
    let trace = std::env::var_os("STELLA_TRACE_THEME_CAMERA").is_some();

    for (index, layer) in bridge.theme_foreground_layers.iter_mut().enumerate() {
        let anchor = match layer.offset_y {
            ThemeVerticalOffset::Top => "top",
            ThemeVerticalOffset::Bottom => "bottom",
            ThemeVerticalOffset::Pixels(_) => {
                layer.resolved_offset_y = None;
                continue;
            }
        };
        layer.resolved_offset_y = None;
        let mut bounds = cameras.iter().map(|camera| {
            native_camera_vertical_bound(
                layer,
                *camera,
                end_scale,
                reference_scale,
                current_scale,
                reference_y,
                orientation,
            )
        });
        let Some(first) = bounds.next() else {
            continue;
        };
        let extreme = match layer.offset_y {
            ThemeVerticalOffset::Top => {
                bounds.fold(
                    first,
                    |selected, value| {
                        if selected < value { value } else { selected }
                    },
                )
            }
            ThemeVerticalOffset::Bottom => {
                bounds.fold(
                    first,
                    |selected, value| {
                        if selected > value { value } else { selected }
                    },
                )
            }
            ThemeVerticalOffset::Pixels(_) => unreachable!(),
        };

        let ratio = end_scale / reference_scale;
        // 0x100098D8C and 0x100098DE8 perform signed integer division before
        // SCVTF, so an odd height truncates toward zero here.
        let half_height = f32::from(native_i16(layer.geometry.height()) / 2);
        let layer_scale_y = layer.scale_y as f32;
        let resolved = match layer.offset_y {
            ThemeVerticalOffset::Top => (-extreme / ratio) + (-half_height * layer_scale_y),
            ThemeVerticalOffset::Bottom => {
                ((screen_height - extreme) / ratio) + (half_height * layer_scale_y)
            }
            ThemeVerticalOffset::Pixels(_) => unreachable!(),
        };
        layer.resolved_offset_y = Some(f64::from(resolved));
        if trace {
            eprintln!(
                "theme foreground anchor: layer={} token={} cameras={} extreme={:.6} resolved={:.6}",
                index + 1,
                anchor,
                cameras.len(),
                extreme,
                resolved
            );
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn native_camera_vertical_bound(
    layer: &ThemeLayer,
    camera: ResolutionCamera,
    end_scale: f32,
    reference_scale: f32,
    current_scale: f32,
    reference_y: f32,
    orientation: f32,
) -> f32 {
    // 0x10009CF1C..0x10009CF54: signed 16-bit height/pivot followed by
    // FNMSUB, with layer+0x40 already zeroed by sub_1000985DC.
    let height = f32::from(native_i16(layer.geometry.height()));
    let pivot_y = f32::from(native_i16(-layer.geometry.min_y));
    let centered_y = height.mul_add(0.5_f32, -pivot_y);
    let local_y = centered_y / reference_scale;
    let camera_ratio = camera.scale / end_scale;
    let z_distance = layer.z_distance as f32;
    let one_minus_z = 1.0_f32 - z_distance;
    let mut projected_y = z_distance.mul_add(local_y / camera_ratio, local_y * one_minus_z);

    let orientation_y = if layer.native_flags & 0x1 != 0 && orientation > 0.0 {
        (-0.5_f32 * orientation) / current_scale
    } else {
        0.0
    };
    projected_y += orientation_y;
    projected_y += reference_y;

    // Foreground mode is zero at sub_10009CFD0, which clears the Y effect
    // register. Only background mode one consumes ThemeManager+0x58.
    let camera_y = if layer.native_flags & 0x20 != 0 {
        camera.y
    } else {
        z_distance * camera.y
    };
    let projected_y = projected_y + camera_y;
    camera.scale * (projected_y - camera.top)
}

fn native_i16(value: f64) -> i16 {
    value as i16
}
