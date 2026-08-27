//! Native two-stage Lua startup and diagnostic instrumentation.

use super::StellaLua;
use crate::*;
use mlua::Function;

impl StellaLua {
    /// Reproduce the native two-stage startup: shared engine logic first,
    /// followed by Stella's game-specific logic.
    pub fn boot(&self, game_script: &str) -> Result<(), ScriptError> {
        // The platform constructor at `sub_100026D2C` loads
        // `scriptPath/starLimits.lua` into a global table named `starTable`
        // before executing gamelogic.lua. It is not imported by either Lua
        // bootstrap chunk, and the first playable level writes into it.
        load_script_to_object(
            &self.lua,
            &self.data_root,
            "scripts/starLimits.lua",
            Some(game_environment(&self.lua)?),
            Some("starTable"),
        )?;
        self.execute("scripts_common/gamelogic.lua")?;
        self.finish_gamelogic_load()?;
        self.install_native_ui_float_precision()?;
        if !matches!(
            game_script,
            "scripts/game.lua" | "scripts_common/gamelogic.lua"
        ) {
            self.execute(game_script)?;
        }
        // IapManager publishes its eight native members before loading the
        // scripts, but registers the six Lua callbacks only once iap.lua has
        // been evaluated. Its provider-success continuation then fetches the
        // wallet and calls onPaymentInitialized in that order.
        complete_iap_initialization(&self.lua)?;
        // The shipped 1.1.6 GameServerConnection chunk deliberately asserts
        // GAMESERVER-DISABLED for every endpoint. Preserve the recovered
        // asynchronous callback shape while making local challenge replay
        // usable without the retired backend.
        install_offline_game_server_facade(&self.lua)?;
        self.install_challenge_result_background_layout()?;
        install_input_queries(&self.lua)?;
        // RovioCloudManager receives all nine native services only after the
        // scripts install its event dispatcher. The announcements load the
        // shipped facades before menus or level components can query them;
        // the Assets facade deliberately calls the separately retained native
        // `_G.Assets`, while the event-only push service has no Lua table.
        announce_cloud_service_registrations(&self.lua)?;
        if std::env::var_os("STELLA_TRACE_UI_INPUT").is_some() {
            self.execute_source(
                r##"
                local originalImageButtonPointerEvent = ui.ImageButton.onPointerEvent
                ui.ImageButton.onPointerEvent = function(self, ...)
                    local values = {...}
                    local rendered = {}
                    for index, value in ipairs(values) do
                        rendered[index] = tostring(value)
                    end
                    print("ui-input", tostring(self.name), table.concat(rendered, ","))
                    local result = {originalImageButtonPointerEvent(self, ...)}
                    print(
                        "ui-state", tostring(self.name),
                        "started=" .. tostring(self.clickStarted),
                        "active=" .. tostring(self.active),
                        "enabled=" .. tostring(self.enabled)
                    )
                    return unpack(result)
                end
                local originalSetAllowInput = menuManager.setAllowInput
                menuManager.setAllowInput = function(self, ...)
                    local values = {...}
                    local rendered = {}
                    for index, value in ipairs(values) do
                        rendered[index] = tostring(value)
                    end
                    print("ui-allow-input", table.concat(rendered, ","))
                    return originalSetAllowInput(self, ...)
                end
                local originalTweenStart = TweenSubsystem.start
                TweenSubsystem.start = function(self, params, ...)
                    print(
                        "ui-tween-start",
                        tostring(params and params.start),
                        tostring(params and params.change),
                        tostring(params and params.duration),
                        tostring(params and params.delay),
                        tostring(params and params.callback),
                        tostring(params and params.doneCallback)
                    )
                    return originalTweenStart(self, params, ...)
                end
                local originalTweenUpdate = TweenSubsystem.update
                local previousTweenCount = -1
                TweenSubsystem.update = function(self, ...)
                    local count = #(self.tweens or {})
                    if count ~= previousTweenCount then
                        print("ui-tween-count", count)
                        previousTweenCount = count
                    end
                    return originalTweenUpdate(self, ...)
                end
                local originalDelegateClicks = menuManager.delegateClicks
                menuManager.delegateClicks = function(self, ...)
                    if not self.__stellaDelegateTrace then
                        self.__stellaDelegateTrace = true
                        print(
                            "ui-delegate-installed",
                            tostring(self), tostring(menuManager),
                            tostring(gamelua.keyPressed), tostring(keyPressed),
                            tostring(gamelua.keyPressed == keyPressed)
                        )
                    end
                    local pressed = isKeyPressed(LBUTTON)
                    local released = isKeyReleased(LBUTTON)
                    local results = {originalDelegateClicks(self, ...)}
                    if pressed or released or gamelua.keyPressed.LBUTTON or gamelua.keyReleased.LBUTTON then
                        print(
                            "ui-delegate",
                            "pressed=" .. tostring(pressed),
                            "released=" .. tostring(released),
                            "allow=" .. tostring(self.allowInput),
                            "root=" .. tostring(self.currentRoot and self.currentRoot.name),
                            "result=" .. tostring(results[1])
                        )
                    end
                    return unpack(results)
                end
                "##,
            )?;
        }
        // sub_100050948 selects a supported localization table and invokes
        // setLocale before the startup-asset callback.
        let environment = game_environment(&self.lua)?;
        if let Value::Function(set_locale) = environment.get::<Value>("setLocale")? {
            set_locale.call::<()>("en_EN")?;
        }
        // Native startup routine sub_10005D44C dispatches this callback after
        // both script layers have been loaded, then installs the five Stella
        // channel limits directly on AudioManager.
        self.initialize_startup_assets()?;
        // ThemeManager construction (`sub_1000985DC`) snapshots the current
        // corrected end-camera scale after the startup camera tables exist.
        self.capture_resolution_camera_scale()?;
        Ok(())
    }

    /// Complete `sub_10005CD58` after its decoded Lua chunk has executed.
    /// Purple invokes the game-specific scalar refresh first and publishes
    /// the loaded byte at +0x513 only if that call returned successfully.
    pub(crate) fn finish_gamelogic_load(&self) -> Result<(), ScriptError> {
        let environment = game_environment(&self.lua)?;
        let update_values = environment.get::<Function>("updateValues")?;
        update_values.call::<()>(())?;
        // GameLua's constructor retains the objects and blockTable LuaObject
        // identities after loading the common game chunk. Later assignment
        // to fields with the same names does not retarget native members.
        retain_constructor_lua_objects(&self.lua)?;
        self.gamelogic_loaded.set(true);
        Ok(())
    }

    /// Complete `sub_10005D44C` in native call order. Purple does not route
    /// these constants back through the Lua adapter: it calls the startup
    /// callback first and only then writes AudioManager tracks 1 through 5.
    pub(crate) fn initialize_startup_assets(&self) -> Result<(), ScriptError> {
        self.call_global("createStartUpAssets")?;
        // sub_10005D44C reloads LuaResources' AudioOutputImpl pointer before
        // each of the five direct AudioManager writes. A successful callback
        // which did not construct an output has no independent limit table.
        crate::resource_manager::require_audio_output(
            &self.resource_runtime,
            "initialize startup audio channel limits",
        )?;
        let mut audio = self
            ._audio_runtime
            .lock()
            .expect("audio runtime lock poisoned");
        audio.channel_limits[1..=5].copy_from_slice(&[4, 6, 3, 5, 5]);
        Ok(())
    }

    /// The shipped challenge result class reuses
    /// `FrenemiesLevelCompleted.layout.lua`, but inherits `ScalableLayout`
    /// directly and consequently misses the 1000x horizontal edge-strip
    /// stretch installed by both ordinary level-result classes. The world is
    /// still drawn underneath this non-fullscreen frame, so the omission
    /// exposes narrow columns of level objects at both drawable edges.
    fn install_challenge_result_background_layout(&self) -> Result<(), ScriptError> {
        self.execute_source(
            r##"
            do
                local resultClass = ui.FrenemiesChallengeLevelCompleted
                if resultClass and not rawget(resultClass, "__stellaBackgroundLayoutRepair") then
                    local inheritedLayout = resultClass.layout
                    resultClass.layout = function(self, ...)
                        inheritedLayout(self, ...)
                        local right = self:getChild("bgStripRight")
                        if right then
                            right:setNonUniformScale(1000, right.scaleY)
                        end
                        local left = self:getChild("bgStripLeft")
                        if left then
                            left:setNonUniformScale(1000, right and right.scaleY or left.scaleY)
                        end
                    end
                    resultClass.__stellaBackgroundLayoutRepair = true
                end
            end
            "##,
        )
    }
}
