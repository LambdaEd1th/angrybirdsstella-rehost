//! Purple's app-wide rating prompt, independent of identity login/logout.

use std::time::{SystemTime, UNIX_EPOCH};

use super::skynest_account::{RegistryNamespace, StoreError};
use crate::*;

mod persistence;
use persistence::RatingStore;

const GAME_VERSION: &str = "1.1.6";
const APP_ID: &str = "875251011";

#[derive(Default)]
struct State {
    next_id: u64,
    prompt: Option<AppRatingPrompt>,
    reason: String,
}

#[derive(Clone)]
pub(crate) struct AppraterRuntime {
    state: Arc<Mutex<State>>,
    path: PathBuf,
    render: Arc<Mutex<RenderBridge>>,
    resources: Arc<Mutex<ResourceRuntime>>,
    locale: Arc<Mutex<LocaleRuntime>>,
    clock: Arc<dyn Fn() -> i64 + Send + Sync>,
}

fn unix_seconds() -> i64 {
    match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(time) => time.as_secs() as i64,
        Err(time) => -(time.duration().as_secs() as i64),
    }
}

impl AppraterRuntime {
    pub(crate) fn prompt(&self) -> Option<AppRatingPrompt> {
        self.state
            .lock()
            .expect("app rating lock poisoned")
            .prompt
            .clone()
    }

    fn show_alert(&self, allowed: bool, reason: String) -> LuaResult<()> {
        let mut state = self.state.lock().expect("app rating lock poisoned");
        let store = RatingStore::open(self.path.clone()).map_err(runtime_error)?;
        // 09D620 calls configure/addTry/needToPrompt BEFORE ANDing the Lua
        // boolean. Even showAlert(false, ...) increments and checks versions.
        store.add_try(&*self.clock).map_err(runtime_error)?;
        let eligible = store
            .need_to_prompt(state.prompt.is_some(), GAME_VERSION, &*self.clock)
            .map_err(runtime_error)?;
        if !eligible || !allowed {
            return Ok(());
        }
        let model = native_device_info_model();
        let message_key = if model.starts_with("iPad") {
            "TEXT_APPRATER_MESSAGE_TITLE"
        } else {
            "TEXT_APPRATER_MESSAGE_TITLE_SHORT"
        };
        let text =
            |key| resolve_localized_string(&self.resources, &self.locale, "TEXTS_BASIC", key);
        let message = text(message_key)?;
        let buttons = [
            AppRatingButton {
                choice: AppRatingChoice::Later,
                title: text("TEXT_APPRATER_RATE_LATER")?,
            },
            AppRatingButton {
                choice: AppRatingChoice::Decline,
                title: text("TEXT_APPRATER_CANCEL_BUTTON")?,
            },
            AppRatingButton {
                choice: AppRatingChoice::Rate,
                title: text("TEXT_APPRATER_RATE_BUTTON")?,
            },
        ];
        state.next_id = state
            .next_id
            .checked_add(1)
            .ok_or_else(|| runtime_error("app rating owner exhausted"))?;
        state.prompt = Some(AppRatingPrompt {
            id: state.next_id,
            message,
            buttons,
        });
        state.reason = reason;
        Ok(())
    }

    pub(crate) fn answer(&self, id: u64, choice: AppRatingChoice) -> LuaResult<bool> {
        let mut state = self.state.lock().expect("app rating lock poisoned");
        if state.prompt.as_ref().is_none_or(|prompt| prompt.id != id) {
            return Ok(false);
        }
        let store = RatingStore::open(self.path.clone()).map_err(runtime_error)?;
        // 40057C timestamps and increments the answer count before analytics,
        // flags, launching the store, and finally clearing the alert owner.
        let count = store.begin_answer(&*self.clock).map_err(runtime_error)?;
        super::analytics::submit(
            &self.render,
            "AppRater".to_owned(),
            BTreeMap::from([
                ("times_seen".to_owned(), count.to_string()),
                (
                    "app_rating_launched".to_owned(),
                    if choice == AppRatingChoice::Rate {
                        "1"
                    } else {
                        "0"
                    }
                    .to_owned(),
                ),
                (
                    "answer".to_owned(),
                    match choice {
                        AppRatingChoice::Rate => "YES",
                        AppRatingChoice::Decline => "NO",
                        AppRatingChoice::Later => "LATER",
                    }
                    .to_owned(),
                ),
                ("shown_because".to_owned(), state.reason.clone()),
            ]),
        );
        store.answer(choice).map_err(runtime_error)?;
        if choice == AppRatingChoice::Rate {
            // Purple's external variant 3, review=true, iOS >= 7.1. The
            // original marks the USER CHOICE before launching; it never
            // receives a completed-rating confirmation from the App Store.
            let url = format!(
                "itms-apps://itunes.apple.com/WebObjects/MZStore.woa/wa/viewContentsUserReviews?onlyLatestVersion=true&pageNumber=0&sortOrdering=1&type=Purple+Software&id={APP_ID}&mt=8&at=10lcoX"
            );
            let mut render = self.render.lock().expect("render bridge lock poisoned");
            render.requested_url = Some(url.clone());
            render
                .platform_action_requests
                .push(PlatformActionRequest::OpenUrl { url });
        }
        state.prompt = None;
        Ok(true)
    }
}

pub(super) fn install(
    lua: &Lua,
    globals: &mlua::Table,
    data_root: &Path,
    render: Arc<Mutex<RenderBridge>>,
    resources: Arc<Mutex<ResourceRuntime>>,
    locale: Arc<Mutex<LocaleRuntime>>,
) -> LuaResult<AppraterRuntime> {
    let runtime = AppraterRuntime {
        state: Arc::default(),
        path: app_data_path(data_root, "stella-installation.registry").map_err(runtime_error)?,
        render,
        resources,
        locale,
        clock: Arc::new(unix_seconds),
    };
    let binding = runtime.clone();
    let table = lua.create_table()?;
    table.set(
        "showAlert",
        lua.create_function(move |_, args: MultiValue| {
            let allowed = native_required_boolean(&args, 0, "Apprater.showAlert")?;
            let reason = native_required_string(&args, 1, "Apprater.showAlert")?;
            binding.show_alert(allowed, reason)
        })?,
    )?;
    globals.set("Apprater", table)?;
    Ok(runtime)
}

#[cfg(test)]
mod tests;
