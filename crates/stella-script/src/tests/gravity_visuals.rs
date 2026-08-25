// Assertions deliberately spell the 3.1416f literal recovered from Purple.
#![allow(clippy::approx_constant)]

use super::*;

fn install_gravity_definitions(runtime: &StellaLua) {
    runtime
        .execute_source(
            r#"
                blockTable = { blocks = {
                    GRAVITY_CIRCLE = { type = "circle" },
                    GRAVITY_BOX = { type = "box" },
                    GRAVITY_POLYGON = { type = "polygon" },
                } }
            "#,
        )
        .unwrap();
}

#[test]
fn gravity_visuals_preserve_native_argument_and_gate_abi() {
    let runtime = StellaLua::new("/tmp").unwrap();
    install_gravity_definitions(&runtime);
    runtime
        .execute_source(
            r#"
                local valid = {
                    sensorType = 17,
                    addVisualTimer = "1.25",
                    active = false,
                    definition = "GRAVITY_POLYGON",
                }
                bad_table = not pcall(renderGravityVisualsNative, 1, 2, 3, 4)
                bad_x = not pcall(renderGravityVisualsNative, valid, "2", 3, 4)
                bad_y = not pcall(renderGravityVisualsNative, valid, 2, {}, 4)
                bad_scale = not pcall(renderGravityVisualsNative, valid, 2, 3, "4")
                renderGravityVisualsNative(valid, 2, 3, 4)
                renderGravityVisualsNative({
                    sensorType = {}, addVisualTimer = 1, active = true,
                    definition = "GRAVITY_CIRCLE"
                }, 2, 3, 4)
            "#,
        )
        .unwrap();
    let environment = game_environment(runtime.lua()).unwrap();
    for field in ["bad_table", "bad_x", "bad_y", "bad_scale"] {
        assert!(environment.get::<bool>(field).unwrap(), "{field}");
    }
    assert!(runtime.take_render_commands().is_empty());
    let state = runtime.render.lock().unwrap().state;
    assert_eq!(state.translate_x, 0.0);
    assert_eq!(state.scale_x, 1.0);
}

#[test]
fn circle_gravity_visuals_draw_two_native_four_way_sprite_rings() {
    let runtime = StellaLua::new("/tmp").unwrap();
    register_test_sprite_sheet_with_sizes(&runtime, &[("GRAVITY_RING", 10, 20)]);
    install_gravity_definitions(&runtime);
    runtime
        .execute_source(
            r#"
                setRenderState(1, 2, 3, 4, 0.1, 6, 7, 0.375)
                res.setClipRect(10, 20, 30, 40)
                renderGravityVisualsNative({
                    sensorType = "radial",
                    addVisualTimer = 0,
                    active = true,
                    definition = "GRAVITY_CIRCLE",
                    radius = "2.5",
                    gravityVisuals = {
                        { scale = "1.25", x = "6.5", y = -7.25, sprite = "GRAVITY_RING" }
                    }
                }, 120.25, -80.5, 2.75)
            "#,
        )
        .unwrap();

    let commands = runtime.take_render_commands();
    assert_eq!(commands.len(), 8);
    let radius_scale = 2.75_f32 * (2.5_f32 * 3.0_f32);
    let factor = (f64::from(radius_scale) * 0.019_f64) as f32;
    let scale = factor * 1.25_f32;
    let diagonal = 3.1416_f32 * 0.25_f32;
    let expected_angles = (0..4)
        .map(|index| ((index as f32) * 3.1416_f32) * 0.5_f32)
        .chain((0..4).map(|index| {
            f64::from((index as f32) * 3.1416_f32).mul_add(0.5, f64::from(diagonal)) as f32
        }))
        .collect::<Vec<_>>();
    for (command, angle) in commands.iter().zip(expected_angles.iter().copied()) {
        assert_eq!(command.sprite, "GRAVITY_RING");
        // Pivot anchoring converts the atlas request to its raw rectangle.
        assert_eq!((command.x, command.y), (1.5, -17.25));
        assert_eq!(command.state.translate_x, f64::from(120.25_f32 / scale));
        assert_eq!(command.state.translate_y, f64::from(-80.5_f32 / scale));
        assert_eq!(command.state.scale_x, f64::from(scale));
        assert_eq!(command.state.scale_y, f64::from(scale));
        assert_eq!((command.state.pivot_x, command.state.pivot_y), (5.0, 10.0));
        assert_eq!(command.state.angle, f64::from(angle));
        assert_eq!(command.state.alpha, f64::from(0.375_f32));
        assert_eq!(command.state.clip_rect, Some([10, 20, 40, 60]));
        assert_eq!(command.state.sprite_pivot, Some([0.0, 0.0]));
        assert!(command.bound_region.is_some());
        assert!(!command.world_space);
    }
    let state = runtime.render.lock().unwrap().state;
    assert_eq!(state.angle, f64::from(*expected_angles.last().unwrap()));
    assert_eq!((state.pivot_x, state.pivot_y), (5.0, 10.0));
    assert_eq!(state.matrix, None);
}

#[test]
fn box_gravity_visuals_use_faded_height_pivot_and_rotated_slice_positions() {
    let runtime = StellaLua::new("/tmp").unwrap();
    register_test_sprite_sheet_with_sizes(
        &runtime,
        &[
            ("THEME_1_GRAVITY_SLICE_BOX_FADED", 8, 12),
            ("GRAVITY_SLICE", 6, 10),
        ],
    );
    install_gravity_definitions(&runtime);
    runtime
        .execute_source(
            r#"
                setRenderState(0, 0, 1, 1, 0, 0, 0, 0.625)
                renderGravityVisualsNative({
                    sensorType = "box",
                    addVisualTimer = 1,
                    active = false,
                    definition = "GRAVITY_BOX",
                    width = "3.5",
                    height = 4.25,
                    angle = "0.4",
                    gravityVisuals = {
                        { pos = "0.75", sprite = "GRAVITY_SLICE" }
                    }
                }, 96.5, -42.25, 1.5)
            "#,
        )
        .unwrap();

    let commands = runtime.take_render_commands();
    assert_eq!(commands.len(), 1);
    let command = &commands[0];
    let local_scale = (3.5_f32 * 20.0_f32) / 12.0_f32;
    let scale = 1.5_f32 * local_scale;
    let (sine, cosine) = 0.4_f32.sin_cos();
    let height_ten = 4.25_f32 * 10.0_f32;
    let y_base = height_ten * cosine;
    let y_delta = (-y_base) - y_base;
    let x_base = height_ten * sine;
    let x_span = (4.25_f32 * 20.0_f32) * sine;
    let expected_x = x_span.mul_add(0.75, -x_base) / local_scale;
    let expected_y = y_delta.mul_add(0.75, y_base) / local_scale;
    assert_eq!(command.sprite, "GRAVITY_SLICE");
    // The slice's own 3x5 pivot is subtracted by AtlasSprite::draw.
    assert_eq!(command.x, f64::from(expected_x) - 3.0);
    assert_eq!(command.y, f64::from(expected_y) - 5.0);
    assert_eq!(command.state.translate_x, f64::from(96.5_f32 / scale));
    assert_eq!(command.state.translate_y, f64::from(-42.25_f32 / scale));
    assert_eq!(command.state.scale_x, f64::from(scale));
    assert_eq!(command.state.scale_y, f64::from(scale));
    let expected_angle = f64::from(3.1416_f32).mul_add(0.5, f64::from(0.4_f32)) as f32;
    assert_eq!(command.state.angle, f64::from(expected_angle));
    // State pivot comes from the faded 8x12 sprite, not the drawn slice.
    assert_eq!((command.state.pivot_x, command.state.pivot_y), (4.0, 6.0));
    assert_eq!(command.state.alpha, f64::from(0.625_f32));
    assert_eq!(command.state.matrix, None);
}

#[test]
fn gravity_visual_arrays_stop_at_first_raw_nil_and_reject_non_table_entries() {
    let runtime = StellaLua::new("/tmp").unwrap();
    register_test_sprite_sheet(&runtime, &["GRAVITY_RING"]);
    install_gravity_definitions(&runtime);
    runtime
        .execute_source(
            r#"
                renderGravityVisualsNative({
                    sensorType = "circle", addVisualTimer = 0, active = true,
                    definition = "GRAVITY_CIRCLE", radius = 1,
                    gravityVisuals = {
                        [1] = { scale = 1, x = 0, y = 0, sprite = "GRAVITY_RING" },
                        [3] = { scale = 1, x = 0, y = 0, sprite = "GRAVITY_RING" },
                    }
                }, 1, 2, 3)
                bad_entry = not pcall(renderGravityVisualsNative, {
                    sensorType = "circle", addVisualTimer = 0, active = true,
                    definition = "GRAVITY_CIRCLE", radius = 1,
                    gravityVisuals = { 17 }
                }, 1, 2, 3)
            "#,
        )
        .unwrap();
    assert_eq!(runtime.take_render_commands().len(), 8);
    assert!(
        game_environment(runtime.lua())
            .unwrap()
            .get::<bool>("bad_entry")
            .unwrap()
    );
}
