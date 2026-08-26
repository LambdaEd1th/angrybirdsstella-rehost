use super::*;

#[test]
fn set_render_state_preserves_fields_outside_native_arity_groups() {
    let runtime = StellaLua::new("/tmp").unwrap();
    register_test_sprite_sheet(
        &runtime,
        &[
            "STATE_FULL",
            "STATE_TRANSLATION_ONLY",
            "STATE_THREE_ARGUMENTS",
            "STATE_ZERO_ARGUMENTS",
        ],
    );
    runtime
        .execute_source(
            r#"
                setRenderState(10, 20, 2, 3, 0.5, 6, 7, 0.25)
                res.drawSprite("STATE_FULL", 0, 0)
                setRenderState(30, 40)
                res.drawSprite("STATE_TRANSLATION_ONLY", 0, 0)
                setRenderState(50, 60, 999)
                res.drawSprite("STATE_THREE_ARGUMENTS", 0, 0)
                setRenderState()
                res.drawSprite("STATE_ZERO_ARGUMENTS", 0, 0)
                "#,
        )
        .unwrap();
    let commands = runtime.take_render_commands();
    assert_eq!(commands.len(), 4);
    assert_eq!(commands[0].state.translate_x, 10.0);
    assert_eq!(commands[0].state.translate_y, 20.0);
    for command in &commands {
        assert_eq!(command.state.scale_x, 2.0);
        assert_eq!(command.state.scale_y, 3.0);
        assert_eq!(command.state.angle, 0.5);
        assert_eq!(command.state.pivot_x, 6.0);
        assert_eq!(command.state.pivot_y, 7.0);
        assert_eq!(command.state.alpha, 0.25);
    }
    assert_eq!(commands[1].state.translate_x, 30.0);
    assert_eq!(commands[1].state.translate_y, 40.0);
    // The native nested branch does not consume a lone third argument.
    assert_eq!(commands[2].state.translate_x, 50.0);
    assert_eq!(commands[2].state.translate_y, 60.0);
    assert_eq!(commands[3].state.translate_x, 50.0);
    assert_eq!(commands[3].state.translate_y, 60.0);
}

#[test]
fn set_render_state_quantizes_and_commits_native_arity_groups() {
    let runtime = StellaLua::new("/tmp").unwrap();
    register_test_sprite_sheet(
        &runtime,
        &[
            "VALID_F32",
            "FAILED_TRANSLATION_PAIR",
            "FAILED_SCALE_PAIR",
            "FAILED_ANGLE",
            "FAILED_PIVOT_PAIR",
            "FAILED_ALPHA",
        ],
    );
    runtime
        .execute_source(
            r#"
                setRenderState(
                    0.99999999, -0.99999999,
                    1.9999999, 2.9999999,
                    0.99999999,
                    3.9999999, 4.9999999,
                    0.99999999
                )
                res.drawSprite("VALID_F32", 0, 0)

                translation_pair_fails = not pcall(setRenderState, 7, "bad")
                res.drawSprite("FAILED_TRANSLATION_PAIR", 0, 0)

                scale_pair_fails = not pcall(setRenderState, 8, 9, 10, "bad")
                res.drawSprite("FAILED_SCALE_PAIR", 0, 0)

                angle_fails = not pcall(setRenderState, 11, 12, 13, 14, "bad")
                res.drawSprite("FAILED_ANGLE", 0, 0)

                pivot_pair_fails = not pcall(
                    setRenderState, 15, 16, 17, 18, 19, 20, "bad"
                )
                res.drawSprite("FAILED_PIVOT_PAIR", 0, 0)

                alpha_fails = not pcall(
                    setRenderState, 21, 22, 23, 24, 25, 26, 27, "bad"
                )
                res.drawSprite("FAILED_ALPHA", 0, 0)
                "#,
        )
        .unwrap();

    let environment = game_environment(runtime.lua()).unwrap();
    for field in [
        "translation_pair_fails",
        "scale_pair_fails",
        "angle_fails",
        "pivot_pair_fails",
        "alpha_fails",
    ] {
        assert!(environment.get::<bool>(field).unwrap(), "{field}");
    }

    let commands = runtime.take_render_commands();
    assert_eq!(commands.len(), 6);
    let valid = commands[0].state;
    assert_eq!(valid.translate_x, 0.99999999_f64 as f32);
    assert_eq!(valid.translate_y, -0.99999999_f64 as f32);
    assert_eq!(valid.scale_x, 1.9999999_f64 as f32);
    assert_eq!(valid.scale_y, 2.9999999_f64 as f32);
    assert_eq!(valid.angle, 0.99999999_f64 as f32);
    assert_eq!(valid.pivot_x, 3.9999999_f64 as f32);
    assert_eq!(valid.pivot_y, 4.9999999_f64 as f32);
    assert_eq!(valid.alpha, 0.99999999_f64 as f32);

    // The failing second translation argument prevents either member of that
    // packed pair from being stored.
    assert_eq!(
        [
            commands[1].state.translate_x,
            commands[1].state.translate_y,
            commands[1].state.scale_x,
            commands[1].state.scale_y,
            commands[1].state.angle,
            commands[1].state.pivot_x,
            commands[1].state.pivot_y,
            commands[1].state.alpha,
        ],
        [
            valid.translate_x,
            valid.translate_y,
            valid.scale_x,
            valid.scale_y,
            valid.angle,
            valid.pivot_x,
            valid.pivot_y,
            valid.alpha,
        ]
    );

    // Later-group failures retain the earlier groups already committed by
    // sub_100044CFC, while preserving the current incomplete pair/scalar.
    assert_eq!(commands[2].state.translate_x, 8.0);
    assert_eq!(commands[2].state.translate_y, 9.0);
    assert_eq!(commands[2].state.scale_x, valid.scale_x);
    assert_eq!(commands[2].state.scale_y, valid.scale_y);

    assert_eq!(commands[3].state.translate_x, 11.0);
    assert_eq!(commands[3].state.translate_y, 12.0);
    assert_eq!(commands[3].state.scale_x, 13.0);
    assert_eq!(commands[3].state.scale_y, 14.0);
    assert_eq!(commands[3].state.angle, valid.angle);

    assert_eq!(commands[4].state.translate_x, 15.0);
    assert_eq!(commands[4].state.translate_y, 16.0);
    assert_eq!(commands[4].state.scale_x, 17.0);
    assert_eq!(commands[4].state.scale_y, 18.0);
    assert_eq!(commands[4].state.angle, 19.0);
    assert_eq!(commands[4].state.pivot_x, valid.pivot_x);
    assert_eq!(commands[4].state.pivot_y, valid.pivot_y);

    assert_eq!(commands[5].state.translate_x, 21.0);
    assert_eq!(commands[5].state.translate_y, 22.0);
    assert_eq!(commands[5].state.scale_x, 23.0);
    assert_eq!(commands[5].state.scale_y, 24.0);
    assert_eq!(commands[5].state.angle, 25.0);
    assert_eq!(commands[5].state.pivot_x, 26.0);
    assert_eq!(commands[5].state.pivot_y, 27.0);
    assert_eq!(commands[5].state.alpha, valid.alpha);
}
