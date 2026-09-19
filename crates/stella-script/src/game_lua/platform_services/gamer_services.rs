//! FusionGamerServices/Game Center ownership and asynchronous events.

use crate::*;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::fs;

const AUTHENTICATION_EVENT: &str = "EID_GS_AUTHENTICATION_STATUS_CHANGED";
const ACHIEVEMENT_EVENT: &str = "EID_GS_POST_ACHIEVEMENT_FINISHED";
const SCORE_EVENT: &str = "EID_GS_POST_SCORE_FINISHED";

#[derive(Clone, Debug)]
enum Completion {
    Achievement { id: String, success: bool },
    Score { id: String, success: bool },
}

/// Main-thread completion state owned by the native Game Center service.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
struct LocalGamerDocument {
    #[serde(default)]
    achievements: BTreeSet<String>,
    #[serde(default)]
    scores: BTreeMap<String, f64>,
}

#[derive(Debug)]
struct GamerServicesState {
    authenticated: bool,
    local_provider: bool,
    persistence_path: PathBuf,
    document: LocalGamerDocument,
    authentication_completions: VecDeque<bool>,
    platform_completions: VecDeque<Completion>,
}

#[derive(Clone, Debug)]
pub(crate) struct GamerServicesRuntime {
    state: Arc<Mutex<GamerServicesState>>,
    render: Arc<Mutex<RenderBridge>>,
    application_events: ApplicationEventScheduler,
}

impl GamerServicesRuntime {
    fn new(
        persistence_path: PathBuf,
        render: Arc<Mutex<RenderBridge>>,
        application_events: ApplicationEventScheduler,
    ) -> Self {
        // GameCenterService installs its authenticate handler while the
        // FusionGamerServices owner is being constructed. GameKit invokes it
        // asynchronously with the current local-player state. A portable
        // desktop host has no GameKit account, so its initial state is false.
        let runtime = Self {
            state: Arc::new(Mutex::new(GamerServicesState {
                authenticated: false,
                local_provider: false,
                persistence_path,
                document: LocalGamerDocument::default(),
                authentication_completions: VecDeque::from([false]),
                platform_completions: VecDeque::new(),
            })),
            render,
            application_events,
        };
        runtime
            .application_events
            .post(ApplicationEvent::GamerServicesAuthentication);
        runtime
    }

    fn pop_authentication(&self) -> Option<bool> {
        self.state
            .lock()
            .expect("gamer-services state lock poisoned")
            .authentication_completions
            .pop_front()
    }

    pub(crate) fn discard_authentication_completion(&self) {
        let _ = self.pop_authentication();
    }

    fn take_platform_completions(&self) -> VecDeque<Completion> {
        std::mem::take(
            &mut self
                .state
                .lock()
                .expect("gamer-services state lock poisoned")
                .platform_completions,
        )
    }

    fn is_authenticated(&self) -> bool {
        self.state
            .lock()
            .expect("gamer-services state lock poisoned")
            .authenticated
    }

    fn persist(state: &GamerServicesState) -> LuaResult<()> {
        if !state.local_provider {
            return Ok(());
        }
        if let Some(parent) = state.persistence_path.parent() {
            fs::create_dir_all(parent).map_err(runtime_error)?;
        }
        let bytes = serde_json::to_vec_pretty(&state.document).map_err(runtime_error)?;
        fs::write(&state.persistence_path, bytes).map_err(runtime_error)
    }

    pub(crate) fn enable_local_provider(&self) -> LuaResult<()> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| runtime_error("gamer-services state lock poisoned"))?;
        if state.persistence_path.is_file() {
            let bytes = fs::read(&state.persistence_path).map_err(runtime_error)?;
            state.document = serde_json::from_slice(&bytes).map_err(runtime_error)?;
        }
        state.local_provider = true;
        state.authenticated = true;
        state.authentication_completions.clear();
        state.authentication_completions.push_back(true);
        Self::persist(&state)?;
        drop(state);
        self.application_events
            .cancel(ApplicationEvent::GamerServicesAuthentication);
        self.application_events
            .post(ApplicationEvent::GamerServicesAuthentication);
        Ok(())
    }

    fn login(&self) {
        let mut state = self
            .state
            .lock()
            .expect("gamer-services state lock poisoned");
        if state.local_provider && !state.authenticated {
            state.authenticated = true;
            state.authentication_completions.push_back(true);
            drop(state);
            self.application_events
                .post(ApplicationEvent::GamerServicesAuthentication);
        }
    }

    fn post_achievement(&self, id: String) -> LuaResult<()> {
        {
            let mut state = self
                .state
                .lock()
                .map_err(|_| runtime_error("gamer-services state lock poisoned"))?;
            if state.local_provider {
                state.document.achievements.insert(id.clone());
                Self::persist(&state)?;
            }
            state
                .platform_completions
                .push_back(Completion::Achievement { id, success: true });
        }
        Ok(())
    }

    fn post_score(&self, id: String, score: f32) -> LuaResult<()> {
        {
            let mut state = self
                .state
                .lock()
                .map_err(|_| runtime_error("gamer-services state lock poisoned"))?;
            if state.local_provider {
                let score = f64::from(score);
                let previous = state.document.scores.entry(id.clone()).or_insert(score);
                *previous = previous.max(score);
                Self::persist(&state)?;
            }
            state
                .platform_completions
                .push_back(Completion::Score { id, success: true });
        }
        Ok(())
    }

    fn show(&self, view: GamerServicesView) {
        let entries = {
            let state = self
                .state
                .lock()
                .expect("gamer-services state lock poisoned");
            if !state.local_provider || !state.authenticated {
                return;
            }
            match view {
                GamerServicesView::Achievements => state
                    .document
                    .achievements
                    .iter()
                    .map(|id| (id.clone(), "Unlocked".to_owned()))
                    .collect(),
                GamerServicesView::Leaderboards => state
                    .document
                    .scores
                    .iter()
                    .map(|(id, score)| (id.clone(), score.to_string()))
                    .collect(),
            }
        };
        self.render
            .lock()
            .expect("render bridge lock poisoned")
            .platform_action_requests
            .push(PlatformActionRequest::ShowGamerServices { view, entries });
    }
}

pub(super) fn install(
    lua: &Lua,
    globals: &mlua::Table,
    persistence_path: PathBuf,
    render: Arc<Mutex<RenderBridge>>,
    application_events: ApplicationEventScheduler,
) -> LuaResult<GamerServicesRuntime> {
    let runtime = GamerServicesRuntime::new(persistence_path, render, application_events);
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
    let authentication_runtime = runtime.clone();
    gamer_services.set(
        "isLocalPlayerAuthenticated",
        lua.create_function(move |_, ()| Ok(authentication_runtime.is_authenticated()))?,
    )?;
    let login_runtime = runtime.clone();
    gamer_services.set(
        "login",
        lua.create_function(move |_, ()| {
            login_runtime.login();
            Ok(())
        })?,
    )?;
    let achievements_runtime = runtime.clone();
    gamer_services.set(
        "showAchievements",
        lua.create_function(move |_, _: MultiValue| {
            achievements_runtime.show(GamerServicesView::Achievements);
            Ok(())
        })?,
    )?;
    let leaderboards_runtime = runtime.clone();
    gamer_services.set(
        "showLeaderboards",
        lua.create_function(move |_, _: MultiValue| {
            leaderboards_runtime.show(GamerServicesView::Leaderboards);
            Ok(())
        })?,
    )?;
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
            achievement_runtime.post_achievement(id)
        })?,
    )?;
    let score_runtime = runtime.clone();
    gamer_services.set(
        "postScore",
        lua.create_function(move |_, args: MultiValue| {
            // sub_1000CC0C4 pairs that STRING accessor with the generated
            // exact NUMBER-tag accessor sub_10052859C for slot two.
            let id = native_required_string(&args, 0, "FusionGamerServices.postScore")?;
            let score = native_required_number(&args, 1, "FusionGamerServices.postScore")? as f32;
            score_runtime.post_score(id, score)
        })?,
    )?;
    globals.set("FusionGamerServices", gamer_services)?;
    Ok(runtime)
}

fn notify_completion(lua: &Lua, completion: Completion) -> LuaResult<()> {
    let environment = game_environment(lua)?;
    let Value::Function(notify_event_manager) = environment.get::<Value>("notifyEventManager")?
    else {
        return Ok(());
    };
    let event = lua.create_table()?;
    let event_name = match completion {
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
    notify_event_manager.call::<()>((event_name, event))
}

/// Deliver Game Center authentication through the process-global application
/// scheduler used by the native authentication handler.
pub(crate) fn dispatch_authentication_completion(
    lua: &Lua,
    runtime: &GamerServicesRuntime,
) -> LuaResult<()> {
    let environment = game_environment(lua)?;
    let Value::Function(notify_event_manager) = environment.get::<Value>("notifyEventManager")?
    else {
        runtime
            .application_events
            .post(ApplicationEvent::GamerServicesAuthentication);
        return Ok(());
    };
    let Some(is_signed_in) = runtime.pop_authentication() else {
        return Ok(());
    };
    let event = lua.create_table()?;
    event.set("isSignedIn", is_signed_in)?;
    notify_event_manager.call::<()>((AUTHENTICATION_EVENT, event))
}

/// GameKit achievement/score completion blocks invoke their retained listener
/// directly on the platform callback lane; they do not join the zero-delay
/// scheduler's cross-service FIFO.
pub(crate) fn dispatch_platform_completions(
    lua: &Lua,
    runtime: &GamerServicesRuntime,
) -> LuaResult<()> {
    let environment = game_environment(lua)?;
    if !matches!(
        environment.get::<Value>("notifyEventManager")?,
        Value::Function(_)
    ) {
        return Ok(());
    }
    for completion in runtime.take_platform_completions() {
        notify_completion(lua, completion)?;
    }
    Ok(())
}
