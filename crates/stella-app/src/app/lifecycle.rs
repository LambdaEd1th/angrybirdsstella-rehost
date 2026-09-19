//! Native application activation and display-link lifetime.

use super::*;

impl StellaApp {
    /// The screenshot CLI never enters winit, so its normal and error returns
    /// need the same final script persistence boundary as `exiting`.
    pub(crate) fn finish_screenshot_run(&mut self, result: Result<()>) -> Result<()> {
        self.application_will_terminate();
        match (result, self.fatal_error.take()) {
            (Ok(()), None) => Ok(()),
            (Err(error), None) => Err(error),
            (Ok(()), Some(error)) => Err(anyhow!(error)),
            (Err(error), Some(termination)) => {
                Err(error.context(format!("screenshot termination also failed: {termination}")))
            }
        }
    }

    fn synchronize_application_audio(&mut self) {
        super::runtime::synchronize_runtime_audio(
            &self.runtime,
            self.audio.as_mut(),
            &mut self.audio_clock,
            Duration::ZERO,
        );
    }

    /// `applicationDidBecomeActive:` snapshots the current monotonic clock
    /// before recreating the display link. Resetting the fixed-step debt here
    /// gives the desktop host the same no-catch-up resume boundary.
    pub(super) fn did_become_active(&mut self) {
        if self.active {
            return;
        }
        self.last_tick = Instant::now();
        self.accumulator = Duration::ZERO;
        // startUpdate calls resetTouches before App slot 19 clears the native
        // hold bytes/touch vector and delivers gameResumed.
        self.reset_platform_input();
        if let Err(error) = self.runtime.set_application_active(true) {
            self.fatal_error = Some(error.to_string());
            return;
        }
        // AppController404D14 posts the SDK resumed event between the two
        // GameApp virtual calls, after Lua gameResumed has returned.
        self.runtime.post_application_resumed();
        if let Err(error) = self.runtime.set_application_audio_active(true) {
            self.fatal_error = Some(error.to_string());
            return;
        }
        self.synchronize_application_audio();
        self.active = true;
    }

    fn stop_application(&mut self, force: bool) {
        if !force && !self.active {
            return;
        }
        self.active = false;
        // stopUpdate calls resetTouches, not viewDidDisappear. The latter
        // would synthesize an LBUTTON release which Purple does not publish
        // when an application simply resigns active.
        self.reset_platform_input();
        if let Err(error) = self.runtime.set_application_active(false) {
            self.fatal_error = Some(error.to_string());
        }
        if let Err(error) = self.runtime.set_application_audio_active(false) {
            self.fatal_error = Some(error.to_string());
        }
        self.synchronize_application_audio();
    }

    /// `applicationWillResignActive:` invalidates Purple's display link. A
    /// desktop focus transition maps to that application callback: reset the
    /// primary owner and discard platform holds/touches without routing it
    /// through the distinct `viewDidDisappear:` release-edge path.
    pub(super) fn will_resign_active(&mut self) {
        self.stop_application(false);
    }

    /// `applicationWillTerminate:` calls `stopUpdate` unconditionally before
    /// destroying AppController. Unlike an ordinary duplicate focus-loss
    /// notification, this must still deliver `gamePaused` after an earlier
    /// resign-active callback so the shipped script performs its final
    /// settings/highscores/BI persistence pass.
    pub(super) fn application_will_terminate(&mut self) {
        self.stop_application(true);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct ShippedDataSandbox {
        root: PathBuf,
        data_root: PathBuf,
    }

    impl ShippedDataSandbox {
        fn new(label: &str) -> Option<Self> {
            let shipped = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../runtime/data");
            if !shipped.join("scripts/game.lua").is_file() {
                return None;
            }
            let unique = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let root = std::env::temp_dir().join(format!(
                "stella-app-{label}-{}-{unique}",
                std::process::id()
            ));
            let data_root = root.join("data");
            fs::create_dir(&root).unwrap();
            fs::create_dir(root.join("appdata")).unwrap();
            let shipped = shipped.canonicalize().unwrap();
            #[cfg(unix)]
            std::os::unix::fs::symlink(shipped, &data_root).unwrap();
            #[cfg(windows)]
            std::os::windows::fs::symlink_dir(shipped, &data_root).unwrap();
            Some(Self { root, data_root })
        }
    }

    impl Drop for ShippedDataSandbox {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    // Release game.lua replaces assert with a no-op. Probes need a lexical
    // assertion, not a replacement of the game's globals or script behavior.
    fn execute_diagnostic_source(
        runtime: &StellaLua,
        source: &str,
    ) -> Result<(), stella_script::ScriptError> {
        runtime.execute_source(&format!(
            r#"
                local raise = _G.error
                local function assert(value, ...)
                    if not value then
                        local message = (...)
                        if message == nil then message = "assertion failed!" end
                        raise(message, 2)
                    end
                    return value, ...
                end
                {source}
            "#
        ))
    }

    #[test]
    fn screenshot_exit_persists_cleared_rewards_and_preserves_failures() {
        let Some(sandbox) = ShippedDataSandbox::new("screenshot-rewards") else {
            return;
        };
        let mut app = StellaApp::new_with_missing_global_diagnostics(
            sandbox.data_root.clone(),
            GameResolution::default(),
            false,
            PlatformServiceOptions::default(),
        )
        .unwrap();
        for failed in [false, true] {
            execute_diagnostic_source(
                &app.runtime,
                r#"
                settings.iap = { gained = {}, used = {}, sync = {} }
                settings.pendingRewards = {}
                Coins:setPendingReward("StarReward_15", 66, "Level end")
                Coins:grantPendingRewards()
                assert(Coins:getAmount() == 66)
                assert(next(SettingsWrapper:getPendingRewards("coins")) == nil)
                loadTableFromFile("settings.lua", "rewardSaveBeforeExit")
                assert(rewardSaveBeforeExit.iap.gained.coins == 66)
                assert(rewardSaveBeforeExit.pendingRewards.coins.StarReward_15.amount == 66)
            "#,
            )
            .unwrap();
            let result = if failed {
                Err(anyhow!("diagnostic failure"))
            } else {
                Ok(())
            };
            let result = app.finish_screenshot_run(result);
            if failed {
                assert!(
                    result
                        .unwrap_err()
                        .to_string()
                        .contains("diagnostic failure")
                );
            } else {
                result.unwrap();
            }
            execute_diagnostic_source(
                &app.runtime,
                r#"
                loadTableFromFile("settings.lua", "rewardSaveAfterExit")
                assert(rewardSaveAfterExit.iap.gained.coins == 66)
                assert(next(rewardSaveAfterExit.pendingRewards.coins) == nil)
                settings = rewardSaveAfterExit
                Coins:grantPendingRewards()
                assert(Coins:getAmount() == 66)
            "#,
            )
            .unwrap();
        }
        app.runtime
            .execute_source("function gamePaused() error('pause-save failure') end")
            .unwrap();
        let error = app.finish_screenshot_run(Ok(())).unwrap_err();
        assert!(error.to_string().contains("pause-save failure"));
        let error = app
            .finish_screenshot_run(Err(anyhow!("original failure")))
            .unwrap_err();
        assert!(format!("{error:#}").contains("original failure"));
        assert!(format!("{error:#}").contains("pause-save failure"));
    }

    #[test]
    fn lifecycle_post_boot_probes_fail_without_enabling_game_assertions() {
        let Some(sandbox) = ShippedDataSandbox::new("lifecycle-probes") else {
            return;
        };
        let app = StellaApp::new_with_missing_global_diagnostics(
            sandbox.data_root.clone(),
            GameResolution::default(),
            false,
            PlatformServiceOptions {
                local_services: true,
                ..PlatformServiceOptions::default()
            },
        )
        .unwrap();
        app.runtime
            .execute_source("lifecycleOriginalAssert = _G.assert; assert(false, 'release no-op')")
            .unwrap();
        let error = execute_diagnostic_source(
            &app.runtime,
            "assert(false, 'lifecycle diagnostic must fail')",
        )
        .unwrap_err();
        assert!(error.to_string().contains("lifecycle diagnostic must fail"));
        execute_diagnostic_source(
            &app.runtime,
            r#"
                assert(_G.assert == lifecycleOriginalAssert)
                assert(select('#', ...) == 0)
                local first, second, third = assert(true, "tail", 42)
                assert(first == true and second == "tail" and third == 42)
                lifecycleDeferredProbe = function()
                    assert(false, "deferred lifecycle diagnostic must fail")
                end
            "#,
        )
        .unwrap();
        let error = app
            .runtime
            .execute_source("lifecycleDeferredProbe()")
            .unwrap_err();
        assert!(
            error
                .to_string()
                .contains("deferred lifecycle diagnostic must fail")
        );
        app.runtime
            .execute_source("_G.assert(false, 'still disabled'); gamelua.assert(false)")
            .unwrap();
    }

    #[test]
    fn termination_forces_final_pause_after_an_already_inactive_transition() {
        let Some(sandbox) = ShippedDataSandbox::new("lifecycle-termination") else {
            return;
        };
        let mut app = StellaApp::new_with_missing_global_diagnostics(
            sandbox.data_root.clone(),
            GameResolution::default(),
            false,
            PlatformServiceOptions {
                local_services: true,
                ..PlatformServiceOptions::default()
            },
        )
        .unwrap();
        execute_diagnostic_source(&app.runtime, "assert(g_showTelepodButtons)").unwrap();
        app.runtime
            .execute_source(
                r#"
                    lifecyclePauseCount = 0
                    lifecycleResumeCount = 0
                    function gamePaused()
                        lifecyclePauseCount = lifecyclePauseCount + 1
                    end
                    function gameResumed()
                        lifecycleResumeCount = lifecycleResumeCount + 1
                    end
                "#,
            )
            .unwrap();

        app.did_become_active();
        execute_diagnostic_source(&app.runtime, "assert(lifecycleResumeCount == 1)").unwrap();
        assert!(app.fatal_error.is_none());
        assert!(app.active);
        app.will_resign_active();
        app.will_resign_active();
        execute_diagnostic_source(&app.runtime, "assert(lifecyclePauseCount == 1)").unwrap();
        assert!(app.fatal_error.is_none());

        app.application_will_terminate();
        execute_diagnostic_source(&app.runtime, "assert(lifecyclePauseCount == 2)").unwrap();
        assert!(app.fatal_error.is_none());
        assert!(!app.active);
        assert!(!app.runtime.audio_output_state().started);
    }

    #[test]
    fn focus_cycle_discards_a_primary_hold_without_publishing_release() {
        let Some(sandbox) = ShippedDataSandbox::new("lifecycle-input") else {
            return;
        };
        let mut app = StellaApp::new_with_missing_global_diagnostics(
            sandbox.data_root.clone(),
            GameResolution::default(),
            false,
            PlatformServiceOptions::default(),
        )
        .unwrap();
        app.runtime
            .execute_source(
                r#"
                    function gamePaused() end
                    function gameResumed() end
                    function update() end
                "#,
            )
            .unwrap();
        app.did_become_active();

        app.cursor = (12.0, 34.0);
        app.cursor_down = true;
        app.primary_touch = Some(7);
        app.touches.push((7, 12, 34));
        app.runtime.set_cursor(12.0, 34.0, true).unwrap();
        app.runtime.set_touches(&app.touches).unwrap();
        // Consume the press edge before the application resigns active.
        assert!(app.runtime.update(0.0).unwrap());
        execute_diagnostic_source(
            &app.runtime,
            "assert(keyHold.LBUTTON); assert(touchcount == 1)",
        )
        .unwrap();

        app.will_resign_active();
        assert!(!app.cursor_down);
        assert!(app.primary_touch.is_none());
        assert!(app.touches.is_empty());
        app.did_become_active();
        assert!(app.runtime.update(0.0).unwrap());
        assert!(app.fatal_error.is_none());
        assert!(app.active);

        execute_diagnostic_source(
            &app.runtime,
            r#"
                assert(not keyHold.LBUTTON)
                assert(not keyReleased.LBUTTON)
                assert(touchcount == 0)
            "#,
        )
        .unwrap();
    }
}
