use std::time::Duration;

use super::*;

fn retained_wav(path: impl Into<std::path::PathBuf>, data_len: u32) -> AudioAssetSource {
    AudioAssetSource::PcmData {
        origin: path.into(),
        data: Arc::from(vec![0; data_len as usize]),
        channels: 1,
        bits_per_sample: 16,
        sample_rate: 16_000,
    }
}

fn install_test_audio_output(runtime: &StellaLua, names: &[&str]) {
    let mut resources = runtime.resource_runtime.lock().unwrap();
    resources.audio_output_created = true;
    resources.audio_output_started = true;
    resources
        .audio_clips
        .extend(names.iter().map(|name| (*name).to_owned()));
}

fn native_rolling_level(angular_speed: f32, radius: f32, mass: f32) -> f32 {
    let radius_scaled = angular_speed * radius;
    let gain_scaled = radius_scaled * f32::from_bits(0x3B23_D70A);
    (gain_scaled * mass).min(1.0_f32)
}

#[test]
fn rolling_audio_levels_use_native_circle_material_max_and_float32_chain() {
    let runtime = StellaLua::new("/tmp").unwrap();
    runtime
        .execute_source(
            r#"
                createCircle("wood", "", 0, 0, 2, 1, 0, 0, true, false, 1)
                createCircle("wood_lower", "", 0, 0, 2, 1, 0, 0, true, false, 1)
                createCircle("rock", "", 0, 0, 3, 1, 0, 0, true, false, 1)
                createCircle("light", "", 0, 0, 4, 1, 0, 0, true, false, 1)
                createCircle("bird", "", 0, 0, 9, 1, 0, 0, true, true, 1)
                createBox("box", "", 0, 0, 9, 9, 1, 0, 0, true, false, 1)
                setMaterial("wood", "wood")
                setMaterial("wood_lower", "wood")
                setMaterial("rock", "stone")
                setMaterial("light", "glass")
                setMaterial("bird", "wood")
                setMaterial("box", "wood")
            "#,
        )
        .unwrap();
    {
        let mut bridge = runtime.render.lock().unwrap();
        let values = [
            ("wood", -4.0, 2.0, 5.0),
            ("wood_lower", 1.0, 2.0, 5.0),
            ("rock", 2.0, 3.0, 4.0),
            ("light", 100.0, 4.0, 8.0),
            ("bird", 100.0, 9.0, 9.0),
            ("box", 100.0, 9.0, 9.0),
        ];
        for (name, angular_velocity, radius, mass) in values {
            let object = bridge.scene.get_mut(name).unwrap();
            object.angular_velocity = angular_velocity;
            object.native_shape_radius = radius;
            object.body_mass = mass;
        }
        let levels = bridge.native_rolling_audio_levels();
        assert_eq!(levels[0], native_rolling_level(4.0, 2.0, 5.0));
        assert_eq!(levels[1], native_rolling_level(2.0, 3.0, 4.0));
        assert_eq!(levels[2], 1.0);
    }
}

#[test]
fn rolling_audio_starts_updates_stops_and_retains_native_handles() {
    let runtime = unlocked_test_runtime();
    install_test_audio_output(&runtime, &["wood_rolling", "rock_rolling", "light_rolling"]);
    runtime
        .execute_source(
            r#"
                createCircle("roller", "", 0, 0, 2, 1, 0, 0, true, false, 1)
                setMaterial("roller", "wood")
                clearLuaForceFunctions = function() end
                update = function() end
            "#,
        )
        .unwrap();
    {
        let mut bridge = runtime.render.lock().unwrap();
        let roller = bridge.scene.get_mut("roller").unwrap();
        roller.angular_velocity = -4.0;
        roller.native_shape_radius = 2.0;
        roller.body_mass = 5.0;
    }

    runtime.update(1.0 / 120.0).unwrap();
    {
        let audio = runtime._audio_runtime.lock().unwrap();
        let clip = &audio.clips[&0];
        assert_eq!(clip.name, "wood_rolling");
        assert_eq!(clip.volume, native_rolling_level(4.0, 2.0, 5.0));
        assert!(clip.looping);
        assert_eq!(clip.channel, 2);
        assert_eq!(audio.next_handle, 1);
    }
    assert_eq!(
        runtime.render.lock().unwrap().rolling_audio_handles,
        [0, 0, 0]
    );

    runtime
        .render
        .lock()
        .unwrap()
        .scene
        .get_mut("roller")
        .unwrap()
        .angular_velocity = 20.0;
    runtime.update(1.0 / 120.0).unwrap();
    {
        let audio = runtime._audio_runtime.lock().unwrap();
        assert_eq!(audio.clips[&0].volume, native_rolling_level(20.0, 2.0, 5.0));
        assert_eq!(audio.next_handle, 1);
    }

    runtime
        .render
        .lock()
        .unwrap()
        .scene
        .get_mut("roller")
        .unwrap()
        .angular_velocity = 0.0;
    runtime.update(1.0 / 120.0).unwrap();
    assert!(runtime._audio_runtime.lock().unwrap().clips.is_empty());
    assert_eq!(
        runtime.render.lock().unwrap().rolling_audio_handles,
        [0, 0, 0]
    );

    let replacement =
        runtime
            ._audio_runtime
            .lock()
            .unwrap()
            .play("wood_rolling".to_owned(), 0.25, true, 2);
    assert_eq!(replacement, 1);
    runtime
        .render
        .lock()
        .unwrap()
        .scene
        .get_mut("roller")
        .unwrap()
        .angular_velocity = 20.0;
    runtime.update(1.0 / 120.0).unwrap();
    // Resource-name playback suppresses a duplicate start, but setVolume
    // still targets GameLua's deliberately stale cached handle zero.
    assert_eq!(
        runtime._audio_runtime.lock().unwrap().clips[&1].volume,
        0.25
    );

    runtime
        .execute_source("setPhysicsEnabled(false, 'pause')")
        .unwrap();
    runtime.update(1.0 / 120.0).unwrap();
    assert!(runtime._audio_runtime.lock().unwrap().clips.is_empty());
}

#[test]
fn startup_assets_callback_precedes_native_stella_channel_limits() {
    let runtime = StellaLua::new("/tmp").unwrap();
    runtime
        .execute_source(
            r#"
                createStartUpAssets = function()
                    setChannelCountLimit(1, 99)
                    startup_callback_observed = true
                end
            "#,
        )
        .unwrap();

    runtime.initialize_startup_assets().unwrap();

    assert!(
        game_environment(runtime.lua())
            .unwrap()
            .get::<bool>("startup_callback_observed")
            .unwrap()
    );
    assert_eq!(
        runtime._audio_runtime.lock().unwrap().channel_limits,
        [-1, 4, 6, 3, 5, 5, -1, -1]
    );
}

#[test]
fn failed_startup_assets_callback_skips_native_channel_limits() {
    let runtime = StellaLua::new("/tmp").unwrap();
    runtime
        .execute_source(
            r#"
                createStartUpAssets = function()
                    setChannelCountLimit(1, 99)
                    error("startup asset failure")
                end
            "#,
        )
        .unwrap();

    let error = runtime.initialize_startup_assets().unwrap_err();

    assert!(error.to_string().contains("startup asset failure"));
    assert_eq!(
        runtime._audio_runtime.lock().unwrap().channel_limits,
        [-1, 99, -1, -1, -1, -1, -1, -1]
    );
}

#[test]
fn resource_audio_playback_matches_native_name_handle_and_output_lifecycle() {
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!("stella-audio-lifecycle-{unique}"));
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join("clip.wav"), test_pcm_wav(32)).unwrap();
    let runtime = StellaLua::new(&root).unwrap();
    runtime
        .execute_source(
            r#"
                fails_without_output = not pcall(function()
                    res.playAudio("CLIP")
                end)
                res.createAudioOutput(2, 16, 44100)
                missing_handle = res.playAudio("MISSING")
                res.createAudio("clip.wav", "CLIP", false)
                stopped_handle = res.playAudio("CLIP")
                res.startAudioOutput()
                handle = res.playAudio("CLIP", 0.25, true, 3.9)
                explicit_nil_volume_rejected = not pcall(function()
                    res.playAudio("CLIP", nil)
                end)
                wrong_loop_type_rejected = not pcall(function()
                    res.playAudio("CLIP", 1, 1)
                end)
                wrong_channel_type_rejected = not pcall(function()
                    res.playAudio("CLIP", 1, false, "3")
                end)
                playing_by_name = res.isAudioPlaying("CLIP")
                playing_by_handle = res.isAudioPlaying(handle)
                res.stopAudio(handle + 0.5)
                fractional_handle_ignored = res.isAudioPlaying(handle)
                res.stopAudio("CLIP")
                stopped_by_name = not res.isAudioPlaying(handle)
                second_handle = res.playAudio("CLIP")
                res.stopAllAudio()
                stopped_all = not res.isAudioPlaying(second_handle)
                "#,
        )
        .unwrap();
    let environment = game_environment(runtime.lua()).unwrap();
    assert!(environment.get::<bool>("fails_without_output").unwrap());
    assert_eq!(environment.get::<i64>("missing_handle").unwrap(), -1);
    assert_eq!(environment.get::<i64>("stopped_handle").unwrap(), -1);
    assert_eq!(environment.get::<i64>("handle").unwrap(), 0);
    assert!(
        environment
            .get::<bool>("explicit_nil_volume_rejected")
            .unwrap()
    );
    assert!(environment.get::<bool>("wrong_loop_type_rejected").unwrap());
    assert!(
        environment
            .get::<bool>("wrong_channel_type_rejected")
            .unwrap()
    );
    assert!(environment.get::<bool>("playing_by_name").unwrap());
    assert!(environment.get::<bool>("playing_by_handle").unwrap());
    assert!(
        environment
            .get::<bool>("fractional_handle_ignored")
            .unwrap()
    );
    assert!(environment.get::<bool>("stopped_by_name").unwrap());
    assert!(environment.get::<bool>("stopped_all").unwrap());
    let _ = fs::remove_dir_all(root);
}

#[test]
fn audio_io_construction_validates_native_formats_and_replaces_output_first() {
    let runtime = StellaLua::new("/tmp").unwrap();
    runtime
        .execute_source(
            r##"
                res.createAudioOutput(2.9, 16.9, 44100.9)
                output_started = res.startAudioOutput()
                setChannelCountLimit(2, 0)
                res.setTrackVolume(0.25, 2)
                res.setMasterVolume(0.5)
                bad_output_channels = not pcall(
                    res.createAudioOutput, 3, 16, 44100
                )
                old_output_was_released = not pcall(res.startAudioOutput)
                bad_output_bits = not pcall(
                    res.createAudioOutput, 2, 24, 44100
                )
                bad_output_rate = not pcall(
                    res.createAudioOutput, 2, 16, 44101
                )
                non_finite_output_fails = not pcall(
                    res.createAudioOutput, 0 / 0, 16, 44100
                )

                res.createAudioInput(1, 8, 12000)
                input_start_results = select("#", res.startAudioInput())
                bad_input_rate = not pcall(
                    res.createAudioInput, 1, 8, 12001
                )
                old_input_was_released = not pcall(res.startAudioInput)
            "##,
        )
        .unwrap();
    let environment = game_environment(runtime.lua()).unwrap();
    for name in [
        "output_started",
        "bad_output_channels",
        "old_output_was_released",
        "bad_output_bits",
        "bad_output_rate",
        "non_finite_output_fails",
        "bad_input_rate",
        "old_input_was_released",
    ] {
        assert!(environment.get::<bool>(name).unwrap(), "{name}");
    }
    assert_eq!(environment.get::<i64>("input_start_results").unwrap(), 0);
    let resources = runtime.resource_runtime.lock().unwrap();
    assert_eq!(resources.audio_output_configuration, None);
    assert_eq!(resources.audio_input_configuration, None);
    assert_eq!(resources.master_volume, -1.0);
    drop(resources);
    let audio = runtime._audio_runtime.lock().unwrap();
    assert_eq!(audio.next_handle, 0);
    assert!(audio.clips.is_empty());
    assert_eq!(audio.track_volumes, [1.0; 8]);
    assert_eq!(audio.channel_limits, [-1; 8]);
}

#[test]
fn audio_output_buffer_matches_native_twenty_five_millisecond_power_of_two() {
    let runtime = StellaLua::new("/tmp").unwrap();
    runtime
        .execute_source("res.createAudioOutput(2, 16, 44100)")
        .unwrap();
    let resources = runtime.resource_runtime.lock().unwrap();
    assert_eq!(
        resources.audio_output_configuration,
        Some(AudioIoConfiguration {
            channels: 2,
            bits_per_sample: 16,
            samples_per_second: 44_100,
            buffer_bytes: 8_192,
        })
    );
}

#[test]
fn replacing_audio_output_reconstructs_manager_state_but_retains_clip_resources() {
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!("stella-audio-output-reset-{unique}"));
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join("clip.wav"), test_pcm_wav(32)).unwrap();

    let runtime = StellaLua::new(&root).unwrap();
    runtime
        .execute_source(
            r#"
                res.createAudioOutput(1, 16, 16000)
                res.createAudio("clip.wav", "CLIP", false)
                res.startAudioOutput()
                setChannelCountLimit(3, 1)
                res.setMasterVolume(0.25)
                res.setTrackVolume(0.5, 3)
                old_handle = res.playAudio("CLIP", 1, true, 3)
                limited = res.playAudio("CLIP", 1, true, 3)

                res.createAudioOutput(1, 16, 16000)
                old_instance_was_destroyed = not res.isAudioPlaying(old_handle)
                track_was_reset = res.getTrackVolume(3)
                res.startAudioOutput()
                reset_handle = res.playAudio("CLIP", 1, true, 3)
                unlimited_again = res.playAudio("CLIP", 1, true, 3)
            "#,
        )
        .unwrap();

    let environment = game_environment(runtime.lua()).unwrap();
    assert_eq!(environment.get::<i64>("old_handle").unwrap(), 0);
    assert_eq!(environment.get::<i64>("limited").unwrap(), -1);
    assert!(
        environment
            .get::<bool>("old_instance_was_destroyed")
            .unwrap()
    );
    assert_eq!(environment.get::<f32>("track_was_reset").unwrap(), 1.0);
    assert_eq!(environment.get::<i64>("reset_handle").unwrap(), 0);
    assert_eq!(environment.get::<i64>("unlimited_again").unwrap(), 1);
    let state = runtime.audio_output_state();
    assert_eq!(state.master_volume, 1.0);
    assert_eq!(state.track_volumes, [1.0; 8]);
    assert_eq!(state.playbacks.len(), 2);
    assert!(
        state
            .playbacks
            .iter()
            .all(|playback| { playback.source == Some(retained_wav(root.join("clip.wav"), 32)) })
    );
    let _ = fs::remove_dir_all(root);
}

#[test]
fn master_volume_uses_the_native_output_pointer_and_first_start_sentinel() {
    let runtime = StellaLua::new("/tmp").unwrap();
    runtime
        .execute_source(
            r##"
                pre_output_results = select("#", res.setMasterVolume(0.25))
                res.createAudioOutput(1, 16, 16000)
            "##,
        )
        .unwrap();
    let environment = game_environment(runtime.lua()).unwrap();
    assert_eq!(environment.get::<i64>("pre_output_results").unwrap(), 0);
    assert_eq!(runtime.resource_runtime.lock().unwrap().master_volume, -1.0);

    runtime
        .execute_source(
            r#"
                res.setMasterVolume(0.4)
                res.startAudioOutput()
            "#,
        )
        .unwrap();
    assert_eq!(runtime.audio_output_state().master_volume, 0.4);

    runtime
        .execute_source(
            r#"
                res.createAudioOutput(1, 16, 16000)
                res.startAudioOutput()
            "#,
        )
        .unwrap();
    assert_eq!(runtime.audio_output_state().master_volume, 1.0);
}

#[test]
fn output_clock_does_not_confuse_reused_handles_across_output_generations() {
    let mut clock = AudioOutputClock::default();
    let playback = |duration| AudioPlaybackState {
        handle: 0,
        source: None,
        duration: Some(duration),
        sample_frames: None,
        source_channels: None,
        source_bits_per_sample: None,
        volume: 1.0,
        looping: false,
        track: 0,
    };
    let mut state = AudioOutputState {
        generation: 1,
        started: true,
        master_volume: 1.0,
        channels: 1,
        bits_per_sample: 16,
        sample_rate: 16_000,
        buffer_bytes: 1_024,
        track_volumes: [1.0; 8],
        playbacks: vec![playback(Duration::from_millis(210))],
    };
    assert!(
        clock
            .synchronize(&state, Duration::from_millis(9))
            .is_empty()
    );

    state.generation = 2;
    state.playbacks = vec![playback(Duration::from_millis(210))];
    assert!(
        clock
            .synchronize(&state, Duration::from_millis(2))
            .is_empty()
    );
    assert!(
        clock
            .synchronize(&state, Duration::from_millis(94))
            .is_empty()
    );
    assert_eq!(clock.synchronize(&state, Duration::from_millis(34)), [0]);
}

#[test]
fn composite_audio_copies_resolved_sequence_until_first_nil() {
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!("stella-audio-composite-{unique}"));
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join("a.wav"), test_pcm_wav(32)).unwrap();
    fs::write(root.join("c.wav"), test_pcm_wav(32)).unwrap();
    let runtime = StellaLua::new(&root).unwrap();
    runtime
        .execute_source(
            r#"
                res.createAudio("a.wav", "A", false)
                res.createAudio("c.wav", "C", false)
                res.createCompositeAudio("SEQUENCE", {
                    "A", "MISSING", "C", [5] = "A"
                })
            "#,
        )
        .unwrap();

    assert_eq!(
        runtime._audio_runtime.lock().unwrap().composite_clips["SEQUENCE"].parts,
        ["A", "C"]
    );
    assert!(
        runtime
            .resource_runtime
            .lock()
            .unwrap()
            .audio_clips
            .contains("SEQUENCE")
    );

    runtime
        .execute_source(r#"res.releaseAudio("SEQUENCE")"#)
        .unwrap();
    assert!(
        !runtime
            ._audio_runtime
            .lock()
            .unwrap()
            .composite_clips
            .contains_key("SEQUENCE")
    );
    let _ = fs::remove_dir_all(root);
}

#[test]
fn audio_output_snapshot_resolves_bundle_files_and_freezes_composite_sources() {
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!("stella-audio-output-{unique}"));
    let data_root = root.join("data");
    let audio_root = data_root.join("audio/sfx");
    fs::create_dir_all(&audio_root).unwrap();
    fs::write(audio_root.join("a.wav"), test_pcm_wav(32)).unwrap();
    fs::write(audio_root.join("b.wav"), test_pcm_wav(32)).unwrap();

    let runtime = StellaLua::new(&data_root).unwrap();
    runtime
        .execute_source(
            r#"
                res.createAudioOutput(2, 16, 44100)
                res.createAudio("audio/sfx/a.wav", "A", false)
                res.createAudio("audio/sfx/b.wav", "B", false)
                res.createCompositeAudio("AB", { "A", "B" })
                res.startAudioOutput()
                handle = res.playAudio("AB", 0.25, true, 3)
                res.setMasterVolume(0.5)
                res.setTrackVolume(0.75, 3)
            "#,
        )
        .unwrap();
    let state = runtime.audio_output_state();
    assert!(state.started);
    assert_eq!(state.master_volume, 0.5);
    assert_eq!(state.track_volumes[3], 0.75);
    assert_eq!(state.playbacks.len(), 1);
    assert_eq!(state.playbacks[0].handle, 0);
    assert_eq!(state.playbacks[0].volume, 0.25);
    assert!(state.playbacks[0].looping);
    assert_eq!(state.playbacks[0].track, 3);
    assert_eq!(
        state.playbacks[0].source,
        Some(AudioAssetSource::Sequence(vec![
            retained_wav(audio_root.join("a.wav"), 32),
            retained_wav(audio_root.join("b.wav"), 32),
        ]))
    );

    runtime.execute_source(r#"res.releaseAudio("A")"#).unwrap();
    assert_eq!(
        runtime.audio_output_state().playbacks[0].source,
        state.playbacks[0].source
    );
    let _ = fs::remove_dir_all(root);
}

#[test]
fn same_name_audio_replacement_stops_direct_instances_but_not_frozen_composites() {
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!("stella-audio-replace-{unique}"));
    let data_root = root.join("data");
    let audio_root = data_root.join("audio/sfx");
    fs::create_dir_all(&audio_root).unwrap();
    fs::write(audio_root.join("old.wav"), test_pcm_wav(32)).unwrap();
    fs::write(audio_root.join("new.wav"), test_pcm_wav(64)).unwrap();

    let runtime = StellaLua::new(&data_root).unwrap();
    runtime
        .execute_source(
            r#"
                res.createAudioOutput(1, 16, 16000)
                res.createAudio("audio/sfx/old.wav", "A", false)
                res.createCompositeAudio("FROZEN", { "A" })
                res.startAudioOutput()
                direct = res.playAudio("A", 1, true, 0)
                composite = res.playAudio("FROZEN", 1, true, 0)
                res.createAudio("audio/sfx/new.wav", "A", false)
                direct_was_stopped = not res.isAudioPlaying(direct)
                composite_survived = res.isAudioPlaying(composite)
                replacement = res.playAudio("A", 1, true, 0)
            "#,
        )
        .unwrap();
    let environment = game_environment(runtime.lua()).unwrap();
    assert!(environment.get::<bool>("direct_was_stopped").unwrap());
    assert!(environment.get::<bool>("composite_survived").unwrap());
    assert_eq!(environment.get::<i64>("replacement").unwrap(), 2);
    let state = runtime.audio_output_state();
    assert_eq!(
        state
            .playbacks
            .iter()
            .find(|playback| playback.handle == 1)
            .and_then(|playback| playback.source.clone()),
        Some(AudioAssetSource::Sequence(vec![retained_wav(
            audio_root.join("old.wav"),
            32,
        )]))
    );
    assert_eq!(
        state
            .playbacks
            .iter()
            .find(|playback| playback.handle == 2)
            .and_then(|playback| playback.source.clone()),
        Some(retained_wav(audio_root.join("new.wav"), 64))
    );
    let _ = fs::remove_dir_all(root);
}

#[test]
fn clip_and_composite_freeze_opened_audio_bytes_before_host_file_replacement() {
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!("stella-audio-frozen-bytes-{unique}"));
    fs::create_dir_all(&root).unwrap();
    let path = root.join("mutable.wav");
    fs::write(&path, test_pcm_wav(32)).unwrap();

    let runtime = StellaLua::new(&root).unwrap();
    runtime
        .execute_source(
            r#"
                res.createAudioOutput(1, 16, 16000)
                res.createAudio("mutable.wav", "OLD", false)
                res.createCompositeAudio("FROZEN", { "OLD" })
            "#,
        )
        .unwrap();
    fs::write(&path, test_pcm_wav(64)).unwrap();
    runtime
        .execute_source(
            r#"
                res.createAudio("mutable.wav", "NEW", true)
            "#,
        )
        .unwrap();
    fs::remove_file(&path).unwrap();
    runtime
        .execute_source(
            r#"
                res.startAudioOutput()
                old_handle = res.playAudio("OLD", 1, true, 0)
                frozen_handle = res.playAudio("FROZEN", 1, true, 0)
                new_handle = res.playAudio("NEW", 1, true, 0)
            "#,
        )
        .unwrap();

    let environment = game_environment(runtime.lua()).unwrap();
    assert_eq!(environment.get::<i64>("old_handle").unwrap(), 0);
    assert_eq!(environment.get::<i64>("frozen_handle").unwrap(), 1);
    assert_eq!(environment.get::<i64>("new_handle").unwrap(), 2);
    let state = runtime.audio_output_state();
    assert_eq!(state.playbacks[0].source, Some(retained_wav(&path, 32)));
    assert_eq!(
        state.playbacks[1].source,
        Some(AudioAssetSource::Sequence(vec![retained_wav(&path, 32)]))
    );
    assert_eq!(
        state.playbacks[2].source,
        Some(AudioAssetSource::EncodedFile {
            path: path.clone(),
            data: Arc::from(test_pcm_wav(64)),
            streaming: true,
            channels: 1,
            bits_per_sample: 16,
            sample_rate: 16_000,
        })
    );
    let _ = fs::remove_dir_all(root);
}

#[test]
fn failed_same_name_audio_construction_preserves_old_pointer_and_instances() {
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!("stella-audio-transaction-{unique}"));
    let audio_root = root.join("audio/sfx");
    fs::create_dir_all(&audio_root).unwrap();
    fs::write(audio_root.join("old.wav"), test_pcm_wav(32)).unwrap();
    fs::write(audio_root.join("invalid.wav"), b"not a wav file").unwrap();

    let runtime = StellaLua::new(&root).unwrap();
    runtime
        .execute_source(
            r#"
                res.createAudioOutput(1, 16, 16000)
                res.createAudio("audio/sfx/old.wav", "A", false)
                res.startAudioOutput()
                old = res.playAudio("A", 1, true, 0)
                invalid_failed = not pcall(
                    res.createAudio, "audio/sfx/invalid.wav", "A", false
                )
                missing_failed = not pcall(
                    res.createAudio, "audio/sfx/missing.wav", "A", false
                )
                old_survived = res.isAudioPlaying(old)
                next = res.playAudio("A", 1, true, 0)
            "#,
        )
        .unwrap();
    let environment = game_environment(runtime.lua()).unwrap();
    assert!(environment.get::<bool>("invalid_failed").unwrap());
    assert!(environment.get::<bool>("missing_failed").unwrap());
    assert!(environment.get::<bool>("old_survived").unwrap());
    assert_eq!(environment.get::<i64>("next").unwrap(), 1);
    let expected = Some(retained_wav(audio_root.join("old.wav"), 32));
    assert!(
        runtime
            .audio_output_state()
            .playbacks
            .iter()
            .all(|playback| playback.source == expected)
    );
    let _ = fs::remove_dir_all(root);
}

#[test]
fn audio_reader_matches_native_magic_extension_raw_and_empty_wav_boundaries() {
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!("stella-audio-detection-{unique}"));
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join("headerless.bin"), [0_u8, 0, 0, 0, 1, 0, 255, 127]).unwrap();
    fs::write(root.join("extension.raw"), [0_u8; 8]).unwrap();
    fs::write(root.join("empty.wav"), b"RIFF\x04\0\0\0WAVE").unwrap();
    fs::write(root.join("ogg_magic_only.bin"), b"OggS\0\0\0\0").unwrap();

    let runtime = StellaLua::new(&root).unwrap();
    runtime
        .execute_source(
            r#"
                res.createAudioOutput(2, 16, 44100)
                res.createAudio("headerless.bin", "RAW", true)
                raw_extension_is_unsupported = not pcall(
                    res.createAudio, "extension.raw", "BAD_RAW", true
                )
                res.createAudio("empty.wav", "EMPTY", false)
                res.createAudio("ogg_magic_only.bin", "OGG_MAGIC_RAW", true)
                res.startAudioOutput()
                raw_handle = res.playAudio("RAW", 1, false, 0)
                empty_handle = res.playAudio("EMPTY", 1, false, 0)
                ogg_magic_handle = res.playAudio("OGG_MAGIC_RAW", 1, false, 0)
            "#,
        )
        .unwrap();

    let environment = game_environment(runtime.lua()).unwrap();
    assert!(
        environment
            .get::<bool>("raw_extension_is_unsupported")
            .unwrap()
    );
    assert_eq!(environment.get::<i64>("raw_handle").unwrap(), 0);
    assert_eq!(environment.get::<i64>("empty_handle").unwrap(), 1);
    assert_eq!(environment.get::<i64>("ogg_magic_handle").unwrap(), 2);

    let state = runtime.audio_output_state();
    assert_eq!(
        state.playbacks[0].source,
        Some(AudioAssetSource::RawPcmFile {
            path: root.join("headerless.bin"),
            data: Arc::from([0_u8, 0, 0, 0, 1, 0, 255, 127]),
            streaming: true,
            channels: 2,
            bits_per_sample: 16,
            sample_rate: 44_100,
        })
    );
    assert_eq!(
        state.playbacks[0].duration,
        Some(Duration::from_nanos(45_351))
    );
    assert_eq!(state.playbacks[1].duration, None);
    assert_eq!(
        state.playbacks[2].source,
        Some(AudioAssetSource::RawPcmFile {
            path: root.join("ogg_magic_only.bin"),
            data: Arc::from(*b"OggS\0\0\0\0"),
            streaming: true,
            channels: 2,
            bits_per_sample: 16,
            sample_rate: 44_100,
        })
    );
    let _ = fs::remove_dir_all(root);
}

#[test]
fn standalone_replacement_commits_over_composite_only_after_decode_succeeds() {
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!("stella-audio-type-swap-{unique}"));
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join("part.wav"), test_pcm_wav(32)).unwrap();
    fs::write(root.join("replacement.wav"), test_pcm_wav(64)).unwrap();
    fs::write(root.join("invalid.wav"), b"not a wav file").unwrap();

    let runtime = StellaLua::new(&root).unwrap();
    runtime
        .execute_source(
            r#"
                res.createAudioOutput(1, 16, 16000)
                res.createAudio("part.wav", "PART", false)
                res.createCompositeAudio("A", { "PART" })
                res.startAudioOutput()
                old = res.playAudio("A", 1, true, 0)
                invalid_failed = not pcall(
                    res.createAudio, "invalid.wav", "A", false
                )
                old_survived_failure = res.isAudioPlaying(old)
                res.createAudio("replacement.wav", "A", false)
                old_stopped_on_commit = not res.isAudioPlaying(old)
                replacement = res.playAudio("A", 1, true, 0)
            "#,
        )
        .unwrap();
    let environment = game_environment(runtime.lua()).unwrap();
    assert!(environment.get::<bool>("invalid_failed").unwrap());
    assert!(environment.get::<bool>("old_survived_failure").unwrap());
    assert!(environment.get::<bool>("old_stopped_on_commit").unwrap());
    assert_eq!(environment.get::<i64>("replacement").unwrap(), 1);
    let audio = runtime._audio_runtime.lock().unwrap();
    assert!(!audio.composite_clips.contains_key("A"));
    assert_eq!(
        audio.clips[&1].asset.as_ref().map(|asset| &asset.source),
        Some(&retained_wav(root.join("replacement.wav"), 64))
    );
    drop(audio);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn same_name_composite_self_reference_freezes_the_previous_clip_pointer() {
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!("stella-audio-self-composite-{unique}"));
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join("old.wav"), test_pcm_wav(32)).unwrap();

    let runtime = StellaLua::new(&root).unwrap();
    runtime
        .execute_source(
            r#"
                res.createAudioOutput(1, 16, 16000)
                res.createAudio("old.wav", "A", false)
                res.startAudioOutput()
                direct = res.playAudio("A", 1, true, 0)
                res.createCompositeAudio("A", { "A" })
                direct_was_stopped = not res.isAudioPlaying(direct)
                replacement = res.playAudio("A", 1, true, 0)
            "#,
        )
        .unwrap();
    let environment = game_environment(runtime.lua()).unwrap();
    assert!(environment.get::<bool>("direct_was_stopped").unwrap());
    assert_eq!(environment.get::<i64>("replacement").unwrap(), 1);
    let audio = runtime._audio_runtime.lock().unwrap();
    assert_eq!(audio.composite_clips["A"].parts, ["A"]);
    assert_eq!(
        audio.clips[&1].asset.as_ref().map(|asset| &asset.source),
        Some(&AudioAssetSource::Sequence(vec![retained_wav(
            root.join("old.wav"),
            32,
        )]))
    );
    drop(audio);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn silent_output_clock_retires_one_shot_and_releases_its_native_channel() {
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!("stella-audio-clock-{unique}"));
    let data_root = root.join("data");
    let audio_root = data_root.join("audio/sfx");
    fs::create_dir_all(&audio_root).unwrap();
    let mut wav = b"RIFF\x48\x01\0\0WAVEfmt \x10\0\0\0".to_vec();
    wav.extend_from_slice(&[1, 0, 1, 0, 0x80, 0x3e, 0, 0, 0, 0x7d, 0, 0, 2, 0, 16, 0]);
    wav.extend_from_slice(b"data\x40\x01\0\0");
    wav.extend_from_slice(&[0; 320]); // 160 mono frames at 16 kHz = 10 ms.
    fs::write(audio_root.join("short.wav"), wav).unwrap();

    let runtime = StellaLua::new(&data_root).unwrap();
    runtime
        .execute_source(
            r#"
                res.createAudioOutput(1, 16, 16000)
                res.createAudio("audio/sfx/short.wav", "SHORT", false)
                res.startAudioOutput()
                setChannelCountLimit(2, 1)
                first = res.playAudio("SHORT", 1, false, 2)
                blocked = res.playAudio("SHORT", 1, false, 2)
            "#,
        )
        .unwrap();
    let environment = game_environment(runtime.lua()).unwrap();
    assert_eq!(environment.get::<i64>("first").unwrap(), 0);
    assert_eq!(environment.get::<i64>("blocked").unwrap(), -1);
    let snapshot = runtime.audio_output_state();
    assert_eq!(
        snapshot.playbacks[0].duration,
        Some(Duration::from_millis(10))
    );

    let mut clock = AudioOutputClock::default();
    // The 10 ms clip is exhausted and removed during the six-buffer start
    // prefill, before any wall-clock time elapses.
    let finished = clock.synchronize(&snapshot, Duration::ZERO);
    assert_eq!(finished, [0]);
    runtime.finish_audio_playbacks(&finished);
    runtime
        .execute_source(
            r#"
                first_finished = not res.isAudioPlaying(first)
                replacement = res.playAudio("SHORT", 1, false, 2)
            "#,
        )
        .unwrap();
    assert!(environment.get::<bool>("first_finished").unwrap());
    assert_eq!(environment.get::<i64>("replacement").unwrap(), 1);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn shipped_boot_resolves_every_retained_audio_clip_to_an_ios_bundle_file() {
    let sandbox = ShippedDataSandbox::new("audio-boot");
    let runtime = StellaLua::new(&sandbox.data_root).unwrap();
    runtime.boot("scripts/game.lua").unwrap();
    runtime
        .execute_source("eventManager:notify({ id = events.EID_START_SCREEN_SEQUENCE_STARTED })")
        .unwrap();
    let resources = runtime.resource_runtime.lock().unwrap();
    let audio = runtime._audio_runtime.lock().unwrap();
    let unresolved = resources
        .audio_clips
        .iter()
        .filter(|name| !audio.assets.contains_key(*name))
        .cloned()
        .collect::<Vec<_>>();
    let unknown_durations = audio
        .assets
        .iter()
        .filter(|(_, asset)| asset.duration.is_none())
        .map(|(name, _)| name.clone())
        .collect::<Vec<_>>();
    let static_encoded = audio
        .assets
        .iter()
        .filter_map(|(name, asset)| match &asset.source {
            AudioAssetSource::EncodedFile {
                streaming: false, ..
            } => Some(name.clone()),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert!(
        unresolved.is_empty(),
        "unresolved retained audio clips: {unresolved:?}"
    );
    assert_eq!(audio.assets.len(), resources.audio_clips.len());
    assert!(audio.assets.len() > 400);
    assert!(
        unknown_durations.is_empty(),
        "audio clips without decoded duration: {unknown_durations:?}"
    );
    assert!(
        static_encoded.is_empty(),
        "non-streaming clips still retaining compressed bytes: {static_encoded:?}"
    );
}

#[test]
fn animation_shader_binding_copies_parameters_and_nil_clears_it() {
    let runtime = StellaLua::new("/tmp").unwrap();
    runtime
        .execute_source(
            r##"
                AnimationWrapperNative.loadFromBundle("scene", "missing.anim.json")
                -- sub_10006CB08 is shared by direct sprite draws and
                -- AnimationWrapper. The missing sprite still mutates the
                -- process shader cache before its resource lookup fails.
                drawSpriteWithShader("MISSING", {
                    name = "2d-sprite-colorize17",
                    params = {
                        { name = "SATURATION", type = "float", value = 0.6 },
                        -- Shipped LEAVES shaders supply RGB only. Native
                        -- defaults the absent alpha component to one.
                        { name = "DIFFUSEC", type = "vector", value = { 0.1, 0.2, 0.3 } }
                    }
                }, 0, 0, 1, 1, 0)
                shader_result_count = select("#", AnimationWrapperNative.setShader("scene", {
                    name = "2d-sprite-colorize17",
                    params = {
                        { name = "LIGHTNESS", type = "float", value = 0.25 }
                    }
                }))
                missing_params_fails = not pcall(
                    AnimationWrapperNative.setShader,
                    "scene",
                    { name = "missing-params" }
                )
                "##,
        )
        .unwrap();
    assert_eq!(
        game_environment(runtime.lua())
            .unwrap()
            .get::<i64>("shader_result_count")
            .unwrap(),
        0
    );
    assert!(
        game_environment(runtime.lua())
            .unwrap()
            .get::<bool>("missing_params_fails")
            .unwrap()
    );
    {
        let animations = runtime._animation_runtime.lock().unwrap();
        let shader = &animations.shaders["scene"];
        assert_eq!(shader.name, "2d-sprite-colorize17");
        assert_eq!(
            shader.diffuse,
            [
                f64::from(0.1_f32),
                f64::from(0.2_f32),
                f64::from(0.3_f32),
                1.0
            ]
        );
        assert_eq!(shader.lightness, f64::from(0.25_f32));
        assert_eq!(shader.saturation, f64::from(0.6_f32));
    }
    runtime
        .execute_source(r#"AnimationWrapperNative.setShader("scene", nil)"#)
        .unwrap();
    assert!(
        runtime
            ._animation_runtime
            .lock()
            .unwrap()
            .shaders
            .is_empty()
    );
    runtime
        .execute_source(
            r#"
                AnimationWrapperNative.setShader("scene", {
                    name = "2d-sprite-colorize17",
                    params = {}
                })
            "#,
        )
        .unwrap();
    {
        let animations = runtime._animation_runtime.lock().unwrap();
        let shader = &animations.shaders["scene"];
        assert_eq!(shader.diffuse[3], 1.0);
        assert_eq!(shader.lightness, f64::from(0.25_f32));
        assert_eq!(shader.saturation, f64::from(0.6_f32));
    }
    runtime
        .execute_source(r#"AnimationWrapperNative.setShader("scene", nil)"#)
        .unwrap();
    assert!(
        !runtime
            .missing_globals()
            .contains(&"AnimationWrapperNative.setShader".to_owned())
    );
}

#[test]
fn sha1_and_unlock_checksum_match_the_native_uppercase_base16_contract() {
    assert_eq!(
        sha1_upper_hex(b"abc"),
        "A9993E364706816ABA3E25717850C26C9CD0D89D"
    );

    let runtime = StellaLua::new("/tmp").unwrap();
    runtime
        .execute_source(
            r#"
                checksum_0 = getUnlockRequestChecksum("A", "B", 0)
                checksum_1 = getUnlockRequestChecksum("A", "B", 1)
                checksum_stack_tail = getUnlockRequestChecksum(
                    "ignored", "A", "B", 0
                )
                checksum_numeric_string_rejected = pcall(
                    getUnlockRequestChecksum, "A", "B", "0"
                )
                checksum_nan_rejected = not pcall(
                    getUnlockRequestChecksum, "A", "B", 0/0
                )
                epoch_utc = getTimeFromEpochSeconds("0")
                epoch_wrong_optional_type = getTimeFromEpochSeconds("0", 1)
                epoch_local = getTimeFromEpochSeconds("0", true)
                epoch_number_rejected = pcall(getTimeFromEpochSeconds, 0)
                "#,
        )
        .unwrap();
    let environment = game_environment(runtime.lua()).unwrap();
    assert_eq!(
        environment.get::<String>("checksum_0").unwrap(),
        "56B3D25B58D86FCE326D20B674DCB9840C4556E7"
    );
    assert_eq!(
        environment.get::<String>("checksum_1").unwrap(),
        "35C2BA63E57D65B080B09E19898A17EBDF327903"
    );
    assert_eq!(
        environment.get::<String>("checksum_stack_tail").unwrap(),
        "56B3D25B58D86FCE326D20B674DCB9840C4556E7"
    );
    assert!(
        !environment
            .get::<bool>("checksum_numeric_string_rejected")
            .unwrap()
    );
    assert!(environment.get::<bool>("checksum_nan_rejected").unwrap());
    for name in ["epoch_utc", "epoch_wrong_optional_type"] {
        let table = environment.get::<mlua::Table>(name).unwrap();
        assert_eq!(table.get::<i32>("year").unwrap(), 1970);
        assert_eq!(table.get::<i32>("month").unwrap(), 1);
        assert_eq!(table.get::<i32>("day").unwrap(), 1);
        assert_eq!(table.get::<i32>("hour").unwrap(), 0);
        assert_eq!(table.get::<i32>("minutes").unwrap(), 0);
        assert_eq!(table.get::<i32>("seconds").unwrap(), 0);
    }
    assert!(environment.get::<mlua::Table>("epoch_local").is_ok());
    assert!(!environment.get::<bool>("epoch_number_rejected").unwrap());
}

#[test]
fn resource_track_volume_uses_native_float_truncation_clamping_and_bounds() {
    let runtime = StellaLua::new("/tmp").unwrap();
    runtime
        .execute_source(
            r#"
                default_volume = res.getTrackVolume(2.9)
                res.setTrackVolume(1.75, 2.9)
                high_volume = res.getTrackVolume(2)
                res.setTrackVolume(-0.5, 3)
                low_volume = res.getTrackVolume(3)
                bounds_ok = not pcall(function() res.getTrackVolume(8) end)
                string_volume_rejected = not pcall(function()
                    res.setTrackVolume("1", 2)
                end)
                string_track_rejected = not pcall(function()
                    res.getTrackVolume("2")
                end)
                "#,
        )
        .unwrap();
    let environment = game_environment(runtime.lua()).unwrap();
    assert_eq!(environment.get::<f64>("default_volume").unwrap(), 1.0);
    assert_eq!(environment.get::<f64>("high_volume").unwrap(), 1.0);
    assert_eq!(environment.get::<f64>("low_volume").unwrap(), 0.0);
    assert!(environment.get::<bool>("bounds_ok").unwrap());
    assert!(environment.get::<bool>("string_volume_rejected").unwrap());
    assert!(environment.get::<bool>("string_track_rejected").unwrap());
}

#[test]
fn complete_purple_registration_gap_is_explicitly_bound() {
    let runtime = StellaLua::new("/tmp").unwrap();
    assert_eq!(resource_manager::REGISTERED_RESOURCE_METHODS.len(), 52);
    let resources = runtime
        .lua()
        .globals()
        .raw_get::<mlua::Table>("res")
        .unwrap();
    for name in resource_manager::REGISTERED_RESOURCE_METHODS {
        assert!(
            matches!(resources.raw_get::<Value>(*name), Ok(Value::Function(_))),
            "Purple LuaResources registration res.{name} is not explicitly bound"
        );
    }
    assert_eq!(
        resource_manager::REGISTERED_LEGACY_RESOURCE_METHODS.len(),
        6
    );
    let legacy_resources = runtime
        .lua()
        .globals()
        .raw_get::<mlua::Table>("ResourceManager")
        .unwrap();
    for name in resource_manager::REGISTERED_LEGACY_RESOURCE_METHODS {
        assert!(
            matches!(
                legacy_resources.raw_get::<Value>(*name),
                Ok(Value::Function(_))
            ),
            "Purple ResourceManager registration ResourceManager.{name} is not explicitly bound"
        );
    }
    assert_eq!(animation_wrapper::REGISTERED_ANIMATION_METHODS.len(), 31);
    let animation = runtime
        .lua()
        .globals()
        .raw_get::<mlua::Table>("AnimationWrapperNative")
        .unwrap();
    for name in animation_wrapper::REGISTERED_ANIMATION_METHODS {
        assert!(
            matches!(animation.raw_get::<Value>(*name), Ok(Value::Function(_))),
            "Purple AnimationWrapper registration AnimationWrapperNative.{name} is not explicitly bound"
        );
    }
    assert_eq!(REGISTERED_GLOBAL_FUNCTIONS.len(), 243);
    assert_eq!(REGISTERED_TABLE_FUNCTIONS.len(), 1);
    for name in REGISTERED_GLOBAL_FUNCTIONS {
        assert!(
            matches!(
                runtime.lua().globals().raw_get::<Value>(*name),
                Ok(Value::Function(_))
            ),
            "Purple native global registration {name} is not callable"
        );
    }
    for (table_name, method_name) in REGISTERED_TABLE_FUNCTIONS {
        let table = runtime
            .lua()
            .globals()
            .raw_get::<mlua::Table>(*table_name)
            .unwrap_or_else(|_| panic!("Purple native table {table_name} is absent"));
        assert!(
            matches!(table.raw_get::<Value>(*method_name), Ok(Value::Function(_))),
            "Purple native table registration {table_name}.{method_name} is not callable"
        );
    }
    assert!(runtime.compatibility_bindings().is_empty());
    for name in [
        "GetDate",
        "addNotificationAfter",
        "checkDirectory",
        "checkInstalledAppsOffline",
        "checkInstalledAppsOnline",
        "checkJointLimits",
        "checkRegistrationResult",
        "createDirectory",
        "createNativeBlockExtension",
        "createThemeAnimation",
        "createThemeSprite",
        "decomposePolygon",
        "destroyTrack",
        "getCurrentTrackAngle",
        "getDeviceID",
        "getDirectoryFileList",
        "getObjectVertices",
        "getTimeFromEpochSeconds",
        "getUnlockRequestChecksum",
        "goToTaskSwitcherLua",
        "handleJointLimits",
        "isGameRenderingDisabled",
        "isInFullScreenMode",
        "isTwitterSupported",
        "linkSensor",
        "loadBlocksForEditing",
        "loadLevelFromAppData",
        "loadTableFromFile",
        "loadTextFileToString",
        "makeLightBeam",
        "makeRay",
        "modifyThemeSprite",
        "multiplyVelocity",
        "native_applySensorForces",
        "native_getLevelLimits",
        "native_resetThemeSystem",
        "native_resizeRadius",
        "native_setBlockCollisionEnabled",
        "native_setIgnoresScore",
        "native_setKeepOrientation",
        "native_setTimeSinceCollision",
        "native_shareScreenShot",
        "native_startURLThread",
        "objectAndTrackOverlap",
        "onLoadLuaFileFail",
        "print",
        "printGlobals",
        "printWithTag",
        "recoverRenderObjects",
        "registerKey",
        "removeAllNotifications",
        "removeJointsFromObject",
        "removeNotification",
        "removeThemeSprite",
        "renderGravityVisualsNative",
        "rotateThemeSprites",
        "saveLevel",
        "saveLuaFile",
        "sendTweet",
        "setGameRenderingDisabled",
        "setJointParameters",
        "setMenuParticlesScale",
        "setNotificationsEnabled",
        "setRecordVelocity",
        "setRevertGravity",
        "setRevertGravityWithMultiplier",
        "setSensorMinimumAndMaximumForces",
        "setSpriteRotation",
        "verifyDeviceID",
    ] {
        assert!(
            matches!(
                runtime.lua().globals().get::<Value>(name),
                Ok(Value::Function(_))
            ),
            "Purple native registration {name} is not explicitly bound"
        );
    }
    runtime
        .execute_source(
            r#"
                assert(select('#', print("message")) == 0)
                assert(select('#', printWithTag("tag", "message")) == 0)
                assert(select('#', createDirectory("unused")) == 0)
                assert(select('#', linkSensor("unused")) == 0)
                assert(select('#', sendTweet("a", "b", "c", "d")) == 0)
                assert(not pcall(drawLayer, "1"))
                assert(select('#', drawLayer(1.9)) == 0)
                assert(type(GetDate()) == "number")
                assert(checkDirectory("unused") == false)
                assert(type(getDirectoryFileList("unused")) == "table")
                assert(getDirectoryFileList("unused") == _G)
                assert(getDirectoryFileList("another/ignored/path") == _G)
                assert(isInFullScreenMode() == true)
                assert(isInFullScreenMode("ignored") == true)
                local urlCallback = function() end
                assert(select('#', native_startURLThread(
                    "https://example.invalid", urlCallback
                )) == 0)
                assert(select('#', native_startURLThread(
                    "https://example.invalid", urlCallback, true
                )) == 0)
                -- Native checks slot 3 only when the total argc is exactly 3.
                assert(pcall(
                    native_startURLThread,
                    "https://example.invalid", urlCallback, "ignored", 4
                ))
                assert(not pcall(print))
                assert(not pcall(printWithTag, "tag"))
                assert(not pcall(createDirectory, false))
                assert(not pcall(sendTweet, "a", "b", "c"))
                assert(not pcall(checkDirectory))
                assert(not pcall(getDirectoryFileList))
                assert(not pcall(getDirectoryFileList, false))
                assert(not pcall(
                    native_startURLThread, "https://example.invalid"
                ))
                assert(not pcall(
                    native_startURLThread, "https://example.invalid", false
                ))
                assert(not pcall(
                    native_startURLThread,
                    "https://example.invalid", urlCallback, "bad"
                ))
                "#,
        )
        .unwrap();
}
