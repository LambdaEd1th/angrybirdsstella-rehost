//! FusionGamerServices/Game Center ownership and asynchronous events.

use crate::*;
use std::collections::VecDeque;

const AUTHENTICATION_EVENT: &str = "EID_GS_AUTHENTICATION_STATUS_CHANGED";
const ACHIEVEMENT_EVENT: &str = "EID_GS_POST_ACHIEVEMENT_FINISHED";
const SCORE_EVENT: &str = "EID_GS_POST_SCORE_FINISHED";

#[derive(Clone, Debug)]
enum Completion {
    Authentication(bool),
    Achievement { id: String, success: bool },
    Score { id: String, success: bool },
}

/// Main-thread completion state owned by the native Game Center service.
#[derive(Clone, Debug)]
pub(crate) struct GamerServicesRuntime {
    completions: Arc<Mutex<VecDeque<Completion>>>,
}

impl Default for GamerServicesRuntime {
    fn default() -> Self {
        // GameCenterService installs its authenticate handler while the
        // FusionGamerServices owner is being constructed. GameKit invokes it
        // asynchronously with the current local-player state. A portable
        // desktop host has no GameKit account, so its initial state is false.
        Self {
            completions: Arc::new(Mutex::new(VecDeque::from([Completion::Authentication(
                false,
            )]))),
        }
    }
}

impl GamerServicesRuntime {
    fn push(&self, completion: Completion) {
        self.completions
            .lock()
            .expect("gamer-services completion queue lock poisoned")
            .push_back(completion);
    }

    fn pop(&self) -> Option<Completion> {
        self.completions
            .lock()
            .expect("gamer-services completion queue lock poisoned")
            .pop_front()
    }
}

pub(super) fn install(lua: &Lua, globals: &mlua::Table) -> LuaResult<GamerServicesRuntime> {
    let runtime = GamerServicesRuntime::default();
    let gamer_services = lua.create_table()?;
    gamer_services.set(
        "isSupported",
        // GameCenter::Impl construction (sub_10054C784) initializes the
        // process-wide availability byte to one. Only an asynchronous
        // GameKit authentication error with code 16 clears it.
        lua.create_function(|_, ()| Ok(true))?,
    )?;
    gamer_services.set(
        "getBackendName",
        // FusionGamerServices::getBackendName (sub_1000CA39C) returns this
        // literal independently of Game Center availability.
        lua.create_function(|_, ()| Ok("gamecenter"))?,
    )?;
    gamer_services.set(
        "isLocalPlayerAuthenticated",
        lua.create_function(|_, ()| Ok(false))?,
    )?;
    for method in ["login", "showAchievements", "showLeaderboards"] {
        gamer_services.set(method, lua.create_function(|_, ()| Ok(()))?)?;
    }
    let achievement_runtime = runtime.clone();
    gamer_services.set(
        "postAchievement",
        lua.create_function(move |_, args: MultiValue| {
            // sub_1000CC2F4 uses the exact STRING-tag accessor
            // sub_1005285CC for slot one and ignores trailing arguments.
            let id = native_required_string(&args, 0, "FusionGamerServices.postAchievement")?;
            // The original reports 100 percent to GameKit. Its completion
            // maps both a nil error and every error on iOS >= 5 to true, which
            // covers every OS capable of running Stella 1.1.6.
            achievement_runtime.push(Completion::Achievement { id, success: true });
            Ok(())
        })?,
    )?;
    let score_runtime = runtime.clone();
    gamer_services.set(
        "postScore",
        lua.create_function(move |_, args: MultiValue| {
            // sub_1000CC0C4 pairs that STRING accessor with the generated
            // exact NUMBER-tag accessor sub_10052859C for slot two.
            let id = native_required_string(&args, 0, "FusionGamerServices.postScore")?;
            let _score = native_required_number(&args, 1, "FusionGamerServices.postScore")? as f32;
            score_runtime.push(Completion::Score { id, success: true });
            Ok(())
        })?,
    )?;
    globals.set("FusionGamerServices", gamer_services)?;
    Ok(runtime)
}

/// Deliver retained GameKit completions through GameLua's native event bridge.
pub(crate) fn dispatch_completions(lua: &Lua, runtime: &GamerServicesRuntime) -> LuaResult<()> {
    let environment = game_environment(lua)?;
    let Value::Function(notify_event_manager) = environment.get::<Value>("notifyEventManager")?
    else {
        // The constructor queues authentication before the shipped bootstrap
        // installs notifyEventManager. Keep it pending until that function is
        // available instead of dropping the native callback.
        return Ok(());
    };

    while let Some(completion) = runtime.pop() {
        let event = lua.create_table()?;
        let event_name = match completion {
            Completion::Authentication(is_signed_in) => {
                event.set("isSignedIn", is_signed_in)?;
                AUTHENTICATION_EVENT
            }
            Completion::Achievement { id, success } => {
                event.set("achievementId", id)?;
                event.set("success", success)?;
                ACHIEVEMENT_EVENT
            }
            Completion::Score { id, success } => {
                event.set("leaderboardId", id)?;
                event.set("success", success)?;
                SCORE_EVENT
            }
        };
        // sub_100065D18 calls the retained GameLua environment member with
        // exactly (eventName, eventTable); notifyEventManager is not a method.
        notify_event_manager.call::<()>((event_name, event))?;
    }
    Ok(())
}
