//! Purple arms a single bird's collision timer after birdCollision returns.

use super::*;

#[test]
fn bird_collision_timer_is_armed_after_the_callback_and_preserves_its_writes() {
    for (second_controllable, initial, callback_value, expected) in [
        (false, -1.0, -2.0, 0.0),
        (false, -1.0, 2.5, 2.5),
        (false, 3.0, -2.0, 0.0),
        (false, 3.0, 2.5, 2.5),
        // FCMP/B.GE at 0x100063918 also clears an unordered callback write.
        (false, 3.0, f64::NAN, 0.0),
        // The two-controllable branch returns directly after its Lua call.
        (true, -1.0, -2.0, -2.0),
    ] {
        let runtime = unlocked_test_runtime();
        runtime
            .execute_source(
                r#"
                    createCircle("bird", "", 0, 0, 1, 1, 0, 0, true, true, 1)
                    createBox("target", "", 3, 0, 2, 4, 0, 0, 0, true, false, 1)
                    objects.world.target.ignoreAllDamage = true
                "#,
            )
            .unwrap();
        {
            let mut bridge = runtime.render.lock().unwrap();
            bridge.scene.get_mut("bird").unwrap().time_since_collision = initial;
            bridge.scene.get_mut("target").unwrap().controllable = second_controllable;
        }
        let environment = game_environment(runtime.lua()).unwrap();
        let render = Arc::clone(&runtime.render);
        environment
            .set(
                "observeCollisionTimer",
                runtime
                    .lua()
                    .create_function(move |_, ()| {
                        Ok(render.lock().unwrap().scene["bird"].time_since_collision)
                    })
                    .unwrap(),
            )
            .unwrap();
        environment.set("callback_timer", callback_value).unwrap();
        runtime
            .execute_source(
                r#"
                    birdCollision = function(first)
                        observed_timer = observeCollisionTimer()
                        native_setTimeSinceCollision(first, callback_timer)
                    end
                "#,
            )
            .unwrap();
        let event = ContactEvent {
            first: "bird".to_owned(),
            second: "target".to_owned(),
            first_fixture: 0,
            second_fixture: 0,
            sensor: false,
            began: true,
            ended: false,
            impulse: 0.0,
            normal_x: 1.0,
            normal_y: 0.0,
            point_x: 2.0,
            point_y: 0.0,
            first_mass: 1.0,
            first_velocity_x: 1.0,
            first_velocity_y: 0.0,
            second_mass: 0.0,
            second_velocity_x: 0.0,
            second_velocity_y: 0.0,
        };
        let callbacks = prepare_native_contact_callbacks(
            runtime.lua(),
            &mut runtime.render.lock().unwrap(),
            &[event],
        )
        .unwrap()
        .0;
        dispatch_native_contact_callbacks(runtime.lua(), &runtime.render, callbacks).unwrap();
        assert_eq!(environment.get::<f64>("observed_timer").unwrap(), initial);
        assert_eq!(
            runtime.render.lock().unwrap().scene["bird"].time_since_collision,
            expected
        );
    }
}
