//! Native block/block BeginContact branch and callback ordering regressions.

use super::*;

fn runtime_with_blocks() -> StellaLua {
    let runtime = unlocked_test_runtime();
    runtime
        .execute_source(
            r#"
                createBox("a", "", 0, 0, 1, 1, 1, 0, 0, true, false, 1)
                createBox("b", "", 0, 0, 1, 1, 1, 0, 0, true, false, 1)
                objects.world.a.strength = 100
                objects.world.b.strength = 100
                worldAttributes = { forceDamageMultiplier = 1, scoreDamageMultiplier = 1 }
                retainedAttributes = worldAttributes
                scoreTable = { blocks = { score = 10.25 } }
                blockCollision = function(...) collision = {...} end
                addScoreToBird = function(value) notified = value end
            "#,
        )
        .unwrap();
    runtime
}

fn prepare(
    runtime: &StellaLua,
    velocity: f64,
) -> mlua::Result<(Vec<NativeContactCallback>, Vec<String>)> {
    let event = ContactEvent {
        first: "a".to_owned(),
        second: "b".to_owned(),
        first_fixture: 0,
        second_fixture: 0,
        sensor: false,
        began: true,
        ended: false,
        impulse: 0.0,
        normal_x: 1.0,
        normal_y: 0.0,
        point_x: 2.0,
        point_y: 3.0,
        first_mass: 1.0,
        first_velocity_x: velocity,
        first_velocity_y: 0.0,
        second_mass: 1.0,
        second_velocity_x: 0.0,
        second_velocity_y: 0.0,
    };
    prepare_native_contact_callbacks(runtime.lua(), &mut runtime.render.lock().unwrap(), &[event])
}

fn dispatch(runtime: &StellaLua, velocity: f64) -> mlua::Result<()> {
    let (callbacks, broken_joints) = prepare(runtime, velocity)?;
    for name in broken_joints {
        dispatch_and_remove_lua_joint(runtime.lua(), &name)?;
    }
    dispatch_native_contact_callbacks(runtime.lua(), &runtime.render, callbacks)
}

#[test]
fn final_damage_argument_preserves_first_hit_when_second_branch_is_skipped() {
    for (setup, damage, damaged, score) in [
        ("", 1.0, true, 5.0),
        ("objects.world.b.ignoreAllDamage = true", 3.0, true, 3.0),
        ("objects.world.b.defence = 99", 3.0, true, 3.0),
        ("objects.world.a.ignoreAllDamage = true", 1.0, true, 1.0),
        ("objects.world.a.defence = 99", 1.0, true, 1.0),
        (
            "objects.world.a.defence = 99; objects.world.b.ignoreAllDamage = true",
            0.0,
            true,
            0.0,
        ),
        (
            "objects.world.a.defence = 99; objects.world.b.defence = 99",
            0.0,
            false,
            0.0,
        ),
        // Equality enters the second branch and overwrites the first payload.
        ("objects.world.b.defence = 3.75", 0.0, true, 3.0),
        // The native lua_isboolean guard rejects truthy numbers and strings.
        ("objects.world.b.ignoreAllDamage = 1", 1.0, true, 5.0),
        ("objects.world.b.ignoreAllDamage = 'true'", 1.0, true, 5.0),
    ] {
        let runtime = runtime_with_blocks();
        runtime
            .execute_source("objects.world.a.defence = 0.5; objects.world.b.defence = 2")
            .unwrap();
        runtime.execute_source(setup).unwrap();
        dispatch(&runtime, 37.5).unwrap();
        let env = game_environment(runtime.lua()).unwrap();
        let collision: mlua::Table = env.get("collision").unwrap();
        assert_eq!(collision.get::<f64>(6).unwrap(), damage, "{setup}");
        assert_eq!(collision.get::<bool>(4).unwrap(), damaged, "{setup}");
        assert_eq!(collision.raw_len(), 10);
        assert_eq!(
            native_capture_block_collision_score(runtime.lua()).unwrap(),
            10.25 + score,
            "{setup}"
        );
    }
}

#[test]
fn destroyed_block_scores_old_strength_without_clamping_callback_raw_damage() {
    let runtime = runtime_with_blocks();
    runtime.execute_source("objects.world.a.strength = 2; objects.world.a.defence = 0.5; objects.world.b.ignoreAllDamage = true").unwrap();
    dispatch(&runtime, 37.5).unwrap();
    runtime
        .execute_source(
            r#"
        assert(collision[6] == 3)
        assert(objects.world.a.strength == -1)
        assert(deadBlocks.a == objects.world.a)
        assert(scoreTable.blocks.score == 12.25 and notified == 2)
    "#,
        )
        .unwrap();
}

#[test]
fn damage_uses_fused_multiply_subtract_but_separately_rounded_threshold() {
    // Binary32 1.3 is below the exact decimal value. The standalone FMUL
    // rounds 10*1.3 to 13, while FNMSUB(10,1.3,12) is just below 1.
    let product = 10.0_f32 * 1.3_f32;
    assert_eq!(product, 13.0);
    assert_eq!(product - 12.0, 1.0);
    assert_eq!(1.3_f32.mul_add(10.0, -12.0), 0.999_999_5);
    for (defence, strength, callback_damage, notified) in [
        (12, 100, 0, "0"),
        // The rounded >= threshold still admits this branch. Fused raw
        // damage is slightly negative, so floor(-epsilon) is -1, not zero.
        (13, 101, -1, "nil"),
    ] {
        let runtime = runtime_with_blocks();
        runtime
            .execute_source(&format!(
                r#"
            objects.world.a.defence = {defence}
            objects.world.a.material = 'wood'
            objects.world.b.ignoreAllDamage = true
            objects.world.b.damageFactors = 'Fused'
            blockTable = {{ damageFactors = {{ Fused = {{
                damageMultiplier = {{ wood = 1.3 }}
            }} }} }}
        "#
            ))
            .unwrap();
        dispatch(&runtime, 100.0).unwrap();
        runtime
            .execute_source(&format!(
                r#"
            assert(objects.world.a.strength == {strength})
            assert(collision[6] == {callback_damage} and collision[4])
            assert(scoreTable.blocks.score == 10.25 and notified == {notified})
        "#
            ))
            .unwrap();
    }
}

#[test]
fn callback_mutations_use_old_score_live_multiplier_and_new_destination() {
    let runtime = runtime_with_blocks();
    runtime
        .execute_source(
            r#"
        oldScoreTable = scoreTable
        addScoreToBird = function() error('stale callback') end
        blockCollision = function()
            assert(objects.world.a.strength == 97 and objects.world.b.strength == 97)
            oldScoreTable.blocks.score = 999
            local destination = setmetatable({}, {
                __index = function() error('score read must be raw') end,
                __newindex = function() error('score write must be raw') end
            })
            scoreTable = { blocks = destination }
            retainedAttributes.scoreDamageMultiplier = '2.9'
            worldAttributes = { scoreDamageMultiplier = 1000 }
            native_setIgnoresScore('a', true)
            native_setIgnoresScore('b', true)
            objects.world.b.strength = 500
            removeObject('a')
            addScoreToBird = function(value)
                assert(value == 14 and scoreTable.blocks.score == 24.25)
                scoreTable.blocks.score = scoreTable.blocks.score + 1000
                notified = value
            end
        end
    "#,
        )
        .unwrap();
    dispatch(&runtime, 37.5).unwrap();
    runtime
        .execute_source(
            r#"
        assert(oldScoreTable.blocks.score == 999)
        assert(scoreTable.blocks.score == 1024.25 and notified == 14)
        assert(objects.world.a == nil and objects.world.b.strength == 500)
    "#,
        )
        .unwrap();
}

#[test]
fn score_snapshot_precedes_joint_removal_callback() {
    let runtime = runtime_with_blocks();
    runtime
        .execute_source(
            r#"
        objects.joints = {}
        createJoint({ name = 'weak', end1 = 'a', end2 = 'b', type = 3,
                      x1 = 0, y1 = 0, x2 = 0, y2 = 0, collideConnected = true })
        lua_onBeforeJointRemove = function(name)
            assert(name == 'weak')
            assert(objects.world.a.strength == 97 and objects.world.b.strength == 97)
            scoreTable.blocks.score = 999
            jointRemoved = true
        end
        blockCollision = function()
            assert(jointRemoved and scoreTable.blocks.score == 999)
            scoreTable.blocks.score = 10000
        end
    "#,
        )
        .unwrap();
    {
        let mut bridge = runtime.render.lock().unwrap();
        let joint = bridge.joints.get_mut("weak").unwrap();
        joint.breakable = true;
        joint.break_force = 0.1;
    }
    dispatch(&runtime, 37.5).unwrap();
    runtime
        .execute_source(
            "assert(jointRemoved and scoreTable.blocks.score == 17.25 and notified == 7)",
        )
        .unwrap();
}

#[test]
fn positive_raw_damage_notifies_even_when_integer_increment_is_zero() {
    for (velocity, multiplier) in [(3.0, "1"), (37.5, "0"), (37.5, "0.75"), (37.5, "nil")] {
        let runtime = runtime_with_blocks();
        runtime
            .execute_source(&format!(
                r#"
            retainedAttributes.scoreDamageMultiplier = {multiplier}
            blockCollision = function() scoreTable.blocks.score = 999 end
        "#
            ))
            .unwrap();
        dispatch(&runtime, velocity).unwrap();
        runtime
            .execute_source("assert(notified == 0 and scoreTable.blocks.score == 10.25)")
            .unwrap();
    }
    let runtime = runtime_with_blocks();
    runtime
        .execute_source("retainedAttributes.scoreDamageMultiplier = -2.9")
        .unwrap();
    dispatch(&runtime, 37.5).unwrap();
    runtime
        .execute_source("assert(notified == -14 and scoreTable.blocks.score == -3.75)")
        .unwrap();
}

#[test]
fn score_capture_is_raw_coercing_and_requires_actual_tables_before_damage() {
    for setup in [
        "scoreTable = nil",
        "scoreTable = { blocks = false }",
        "scoreTable = setmetatable({}, { __index = function() error('metamethod') end })",
    ] {
        let runtime = runtime_with_blocks();
        runtime.execute_source(setup).unwrap();
        let error = prepare(&runtime, 37.5).unwrap_err().to_string();
        assert!(error.contains("Tried to get a Lua table"), "{error}");
        runtime
            .execute_source(
                "assert(objects.world.a.strength == 100 and objects.world.b.strength == 100)",
            )
            .unwrap();
    }
    for (value, expected) in [
        ("' 12.5 '", 19.5),
        ("true", 7.0),
        ("nil", 7.0),
        ("16777217", 16777224.0),
    ] {
        let runtime = runtime_with_blocks();
        runtime.execute_source(&format!("scoreTable.blocks = setmetatable({{ score = {value} }}, {{ __index = function() error('raw read') end, __newindex = function() error('raw write') end }})")).unwrap();
        dispatch(&runtime, 37.5).unwrap();
        assert_eq!(
            native_capture_block_collision_score(runtime.lua()).unwrap(),
            expected
        );
    }
}

#[test]
fn damage_fields_ignore_metamethods_and_use_native_number_and_boolean_rules() {
    let runtime = runtime_with_blocks();
    runtime
        .execute_source(
            r#"
        objects.world.a.strength = '10.5'
        objects.world.a.defence = '1.5'
        objects.world.a.ignoreAllDamage = 1
        setmetatable(objects.world.a, {
            __index = function() error('raw damage lookup') end,
            __newindex = function() error('raw damage write') end
        })
    "#,
        )
        .unwrap();
    native_apply_collision_damage(runtime.lua(), "a", 3.75).unwrap();
    runtime
        .execute_source("assert(objects.world.a.strength == 8.5)")
        .unwrap();
    runtime.execute_source("rawset(objects.world.a, 'strength', nil); rawset(objects.world.a, 'defence', nil); rawset(objects.world.a, 'ignoreAllDamage', nil)").unwrap();
    native_apply_collision_damage(runtime.lua(), "a", 0.5).unwrap();
    runtime
        .execute_source(
            "assert(rawget(objects.world.a, 'strength') == 0 and deadBlocks.a == objects.world.a)",
        )
        .unwrap();
    runtime
        .execute_source(
            "rawset(objects.world.a, 'defence', 0/0); rawset(objects.world.a, 'strength', 100)",
        )
        .unwrap();
    native_apply_collision_damage(runtime.lua(), "a", 1.0).unwrap();
    runtime
        .execute_source("assert(objects.world.a.strength == 100)")
        .unwrap();
}

#[test]
fn callback_errors_preserve_completed_phases_and_stop_later_phases() {
    let runtime = runtime_with_blocks();
    runtime
        .execute_source("blockCollision = function() error('block failed') end")
        .unwrap();
    assert!(
        dispatch(&runtime, 37.5)
            .unwrap_err()
            .to_string()
            .contains("block failed")
    );
    runtime.execute_source("assert(objects.world.a.strength == 97 and scoreTable.blocks.score == 10.25 and notified == nil)").unwrap();

    let runtime = runtime_with_blocks();
    runtime
        .execute_source("blockCollision = function() scoreTable = nil end")
        .unwrap();
    assert!(
        dispatch(&runtime, 37.5)
            .unwrap_err()
            .to_string()
            .contains("scoreTable")
    );
    runtime
        .execute_source("assert(objects.world.a.strength == 97 and notified == nil)")
        .unwrap();

    let runtime = runtime_with_blocks();
    runtime
        .execute_source("addScoreToBird = function(value) error('notify failed') end")
        .unwrap();
    assert!(
        dispatch(&runtime, 37.5)
            .unwrap_err()
            .to_string()
            .contains("notify failed")
    );
    runtime
        .execute_source("assert(scoreTable.blocks.score == 17.25)")
        .unwrap();
}

#[test]
fn nonpositive_raw_damage_skips_post_callback_table_and_notification_lookup() {
    let runtime = runtime_with_blocks();
    runtime
        .execute_source(
            r#"
        objects.world.a.ignoreAllDamage = true
        objects.world.b.ignoreAllDamage = true
        blockCollision = function() scoreTable = nil end
        addScoreToBird = function() error('must not notify') end
    "#,
        )
        .unwrap();
    dispatch(&runtime, 37.5).unwrap();
    // FCMP/B.LE rejects unordered score damage as well as zero/negative.
    native_add_block_collision_score(runtime.lua(), 10.25, f64::NAN).unwrap();
    native_add_block_collision_score(runtime.lua(), 10.25, -1.0).unwrap();
}
