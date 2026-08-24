use super::*;

#[test]
fn darwin_libc_random_matches_ios_seed_edges() {
    let mut random = NativeLibcRandom::default();
    assert_eq!(
        (0..5).map(|_| random.next_word()).collect::<Vec<_>>(),
        [
            16_807,
            282_475_249,
            1_622_650_073,
            984_943_658,
            1_144_108_930
        ]
    );

    random.seed(0);
    assert_eq!(
        (0..5).map(|_| random.next_word()).collect::<Vec<_>>(),
        [
            520_932_930,
            28_925_691,
            822_784_415,
            890_459_872,
            145_532_761
        ]
    );

    random.seed(0x7fff_ffff);
    assert_eq!(random.next_word(), 0);
    assert_eq!(random.next_word(), 520_932_930);
    random.seed(0xffff_ffff);
    assert_eq!(random.next_word(), 16_807);
}

#[test]
fn lua_math_random_uses_purple_float32_results_and_intervals() {
    let runtime = StellaLua::new("/tmp").unwrap();
    runtime
        .execute_source(
            r#"
                math.randomseed(1)
                random_zero_arg_1 = math.random()
                random_zero_arg_2 = math.random()
                math.randomseed(1)
                random_upper = math.random(10)
                math.randomseed(1)
                random_range = math.random(-3, 3)
                math.randomseed("2", "ignored")
                random_string_seed = math.random()
            "#,
        )
        .unwrap();

    let globals = game_environment(runtime.lua()).unwrap();
    assert_eq!(
        globals.get::<f64>("random_zero_arg_1").unwrap().to_bits(),
        f64::from(f32::from_bits(0x3703_4e00)).to_bits()
    );
    assert_eq!(
        globals.get::<f64>("random_zero_arg_2").unwrap().to_bits(),
        f64::from(f32::from_bits(0x3e06_b1d8)).to_bits()
    );
    assert_eq!(globals.get::<f64>("random_upper").unwrap(), 1.0);
    assert_eq!(globals.get::<f64>("random_range").unwrap(), -3.0);
    assert_eq!(
        globals.get::<f64>("random_string_seed").unwrap(),
        f64::from((33_614_f32) * f32::from_bits(0x3000_0000))
    );
}

#[test]
fn failed_math_random_calls_still_advance_the_native_stream() {
    let runtime = StellaLua::new("/tmp").unwrap();
    runtime
        .execute_source(
            r#"
                math.randomseed(1)
                local ok_empty, error_empty = pcall(math.random, 0)
                after_empty = math.random()
                math.randomseed(1)
                local ok_arity, error_arity = pcall(math.random, 1, 2, 3)
                after_arity = math.random()
                empty_failed = not ok_empty and string.find(tostring(error_empty), "interval is empty", 1, true) ~= nil
                arity_failed = not ok_arity and string.find(tostring(error_arity), "wrong number of arguments", 1, true) ~= nil
            "#,
        )
        .unwrap();

    let globals = game_environment(runtime.lua()).unwrap();
    assert!(globals.get::<bool>("empty_failed").unwrap());
    assert!(globals.get::<bool>("arity_failed").unwrap());
    let expected_second = f64::from(f32::from_bits(0x3e06_b1d8));
    assert_eq!(globals.get::<f64>("after_empty").unwrap(), expected_second);
    assert_eq!(globals.get::<f64>("after_arity").unwrap(), expected_second);
}
