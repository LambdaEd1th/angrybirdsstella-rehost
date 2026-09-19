//! Purple's process-global delayed application callback scheduler.

use crate::*;
use std::collections::VecDeque;

const DISPATCH_REGISTRY_KEY: &str = "stella.application-events.dispatch";
const POST_ENTITY_REMOVAL_REGISTRY_KEY: &str = "stella.application-events.remove-entity";
pub(crate) const ANIMATION_ENTITY_REMOVAL_REGISTRY_KEY: &str = "stella.animation.remove-entity";
const POST_ENTITY_ATTACHMENT_REGISTRY_KEY: &str = "stella.application-events.attach-entity";
pub(crate) const ANIMATION_ENTITY_ATTACHMENT_REGISTRY_KEY: &str = "stella.animation.attach-entity";

/// Completion lanes that post through Purple's process-global delayed
/// scheduler. Payloads remain owned by their native-service counterparts;
/// this queue records the single cross-service insertion order.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ApplicationEvent {
    Url,
    GameServer,
    ServerTime,
    GamerServicesAuthentication,
    SocialLocal,
    SocialOnline,
    Assets,
    ChannelLoadingFailure,
    ChannelContent,
    Iap,
    IapInitializationRetry(u64),
    QrRecognition,
    SkynestAccountLocal,
    SkynestAccountOnline,
    SkynestSessionSuccess,
    ApplicationResumed,
    SdkLogFlush(u64),
    SkynestStorageLocal,
    SkynestStorageOnline,
    AnimationEntityRemoval(u64),
    AnimationEntityAttachment(u64),
}

#[derive(Clone, Copy, Debug)]
struct ScheduledEvent {
    event: ApplicationEvent,
    remaining: f32,
}

#[derive(Debug, Default)]
struct ApplicationEventState {
    /// Worker-safe insertion half guarded by `unk_100C28930` in
    /// `sub_10057C1BC`.
    pending: VecDeque<ScheduledEvent>,
    /// Persistent active vector and pre-advanced cursor from
    /// `sub_10057C418`. Keeping these separate from `pending` is what lets a
    /// nested AnimationWrapper drain continue the outer walk.
    active: Vec<ScheduledEvent>,
    cursor: usize,
}

/// Per-GameLua representation of Purple's process-global scheduler.
///
/// Stella ships one AppController/GameLua pair, so this has the same observable
/// lifetime as the native static. Keeping ownership with that host also avoids
/// letting one embedded/test Lua VM consume another VM's service payloads.
#[derive(Clone, Debug, Default)]
pub(crate) struct ApplicationEventScheduler {
    state: Arc<Mutex<ApplicationEventState>>,
}

impl ApplicationEventScheduler {
    pub(crate) fn post(&self, event: ApplicationEvent) {
        self.post_delayed(event, 0.0);
    }

    pub(crate) fn post_delayed(&self, event: ApplicationEvent, delay: f32) {
        self.state
            .lock()
            .expect("application event queue lock poisoned")
            .pending
            .push_back(ScheduledEvent {
                event,
                remaining: delay,
            });
    }

    /// Append the complete pending FIFO to the persistent active vector.
    ///
    /// Callbacks run without this lock. A worker post therefore cannot
    /// deadlock against a service payload queue, and a nested drain can append
    /// everything posted by its outer callback before resuming at the shared
    /// cursor.
    fn begin_dispatch(&self) {
        let mut state = self
            .state
            .lock()
            .expect("application event queue lock poisoned");
        let pending = std::mem::take(&mut state.pending);
        state.active.extend(pending);
    }

    /// Advance before returning the event, matching the store to
    /// `qword_100C28938` at `0x10057C60C` before the callback invocation.
    fn next_active(&self, delta: f32) -> Option<ApplicationEvent> {
        let mut state = self
            .state
            .lock()
            .expect("application event queue lock poisoned");
        while state.cursor < state.active.len() {
            let index = state.cursor;
            state.cursor += 1;
            let entry = &mut state.active[index];
            // 0x10057C618 subtracts S0 in single precision on each visit.
            // A positive timer does not block later zero-delay callbacks.
            entry.remaining -= delta;
            if entry.remaining <= 0.0 {
                return Some(entry.event);
            }
        }
        None
    }

    fn finish_dispatch(&self) {
        let mut state = self
            .state
            .lock()
            .expect("application event queue lock poisoned");
        if state.cursor == state.active.len() {
            state.active.retain(|entry| entry.remaining > 0.0);
            state.cursor = 0;
        }
    }

    /// Abandon only the unvisited active tail after the Rust host frame returns
    /// an otherwise-fatal callback error. Newly posted work stays in `pending`.
    fn take_unvisited_active(&self) -> Vec<ApplicationEvent> {
        let mut state = self
            .state
            .lock()
            .expect("application event queue lock poisoned");
        let cursor = state.cursor;
        let unvisited = state
            .active
            .drain(cursor..)
            .map(|entry| entry.event)
            .collect();
        // Visited positive timers have not invoked or consumed their payload.
        state.active.retain(|entry| entry.remaining > 0.0);
        state.cursor = 0;
        unvisited
    }

    pub(crate) fn cancel(&self, event: ApplicationEvent) {
        let mut state = self
            .state
            .lock()
            .expect("application event queue lock poisoned");
        state.pending.retain(|candidate| candidate.event != event);
        // A service cancellation also invalidates its payload. Remove every
        // not-yet-invoked token, including ones already appended to `active`,
        // so an old request cannot consume a newly posted replacement. Keep
        // already invoked prefix intact. Visited positive timers still own
        // future work, so cancel those too and adjust the shared cursor.
        let cursor = state.cursor;
        let mut index = 0;
        let mut removed_prefix = 0;
        state.active.retain(|candidate| {
            let keep = candidate.event != event || (index < cursor && candidate.remaining <= 0.0);
            if !keep && index < cursor {
                removed_prefix += 1;
            }
            index += 1;
            keep
        });
        state.cursor -= removed_prefix;
    }
}

/// One cloneable dispatch view over all payload lanes belonging to a GameLua.
/// The scheduler itself remains worker-safe; this view is invoked only on the
/// Lua thread, either from AppController's frame head or from a native
/// AnimationWrapper scheduler call site.
#[derive(Clone)]
pub(crate) struct ApplicationEventDispatcher {
    scheduler: ApplicationEventScheduler,
    url_requests: UrlRequestRuntime,
    game_server: GameServerRuntime,
    server_time: ServerTimeRuntime,
    gamer_services: GamerServicesRuntime,
    social: SocialRuntime,
    assets: AssetsRuntime,
    channel: ChannelRuntime,
    iap: IapRuntime,
    qr_scanner: QrScannerRuntime,
    skynest_account: SkynestAccountRuntime,
    skynest_storage: SkynestStorageRuntime,
}

impl ApplicationEventDispatcher {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        scheduler: ApplicationEventScheduler,
        url_requests: UrlRequestRuntime,
        game_server: GameServerRuntime,
        server_time: ServerTimeRuntime,
        gamer_services: GamerServicesRuntime,
        social: SocialRuntime,
        assets: AssetsRuntime,
        channel: ChannelRuntime,
        iap: IapRuntime,
        qr_scanner: QrScannerRuntime,
        skynest_account: SkynestAccountRuntime,
        skynest_storage: SkynestStorageRuntime,
    ) -> Self {
        Self {
            scheduler,
            url_requests,
            game_server,
            server_time,
            gamer_services,
            social,
            assets,
            channel,
            iap,
            qr_scanner,
            skynest_account,
            skynest_storage,
        }
    }

    fn dispatch_one(&self, lua: &Lua, event: ApplicationEvent) -> LuaResult<()> {
        match event {
            ApplicationEvent::Url => dispatch_url_completion(lua, &self.url_requests),
            ApplicationEvent::GameServer => dispatch_game_server_completion(lua, &self.game_server),
            ApplicationEvent::ServerTime => dispatch_server_time_completion(lua, &self.server_time),
            ApplicationEvent::GamerServicesAuthentication => {
                dispatch_gamer_services_authentication_completion(lua, &self.gamer_services)
            }
            ApplicationEvent::SocialLocal => dispatch_social_local_completion(lua, &self.social),
            ApplicationEvent::SocialOnline => dispatch_social_online_completion(lua, &self.social),
            ApplicationEvent::Assets => dispatch_assets_completion(lua, &self.assets),
            ApplicationEvent::ChannelLoadingFailure => {
                dispatch_channel_loading_failure(lua, &self.channel)
            }
            ApplicationEvent::ChannelContent => {
                dispatch_channel_content_completion(lua, &self.channel)
            }
            ApplicationEvent::Iap => dispatch_iap_completion(lua, &self.iap),
            ApplicationEvent::IapInitializationRetry(generation) => {
                self.iap.retry_initialization(lua, generation)
            }
            ApplicationEvent::SkynestSessionSuccess => {
                if let Some(lifetime) = self.skynest_account.pop_session_success() {
                    self.iap.session_succeeded(lua, lifetime.clone())?;
                    self.social.session_succeeded(lua, lifetime)?;
                }
                Ok(())
            }
            ApplicationEvent::ApplicationResumed => self.social.application_resumed(lua),
            ApplicationEvent::SdkLogFlush(generation) => {
                self.skynest_account.flush_sdk_log_timer(generation);
                Ok(())
            }
            ApplicationEvent::QrRecognition => dispatch_qr_completion(lua, &self.qr_scanner),
            ApplicationEvent::SkynestAccountLocal => {
                dispatch_skynest_account_local_completion(lua, &self.skynest_account)
            }
            ApplicationEvent::SkynestAccountOnline => {
                dispatch_skynest_account_online_completion(lua, &self.skynest_account)
            }
            ApplicationEvent::SkynestStorageLocal => {
                dispatch_skynest_storage_local_completion(lua, &self.skynest_storage)
            }
            ApplicationEvent::SkynestStorageOnline => {
                dispatch_skynest_storage_online_completion(lua, &self.skynest_storage)
            }
            ApplicationEvent::AnimationEntityRemoval(generation) => lua
                .named_registry_value::<mlua::Function>(ANIMATION_ENTITY_REMOVAL_REGISTRY_KEY)?
                .call(generation),
            ApplicationEvent::AnimationEntityAttachment(generation) => lua
                .named_registry_value::<mlua::Function>(ANIMATION_ENTITY_ATTACHMENT_REGISTRY_KEY)?
                .call(generation),
        }
    }

    pub(crate) fn dispatch(&self, lua: &Lua, delta: f32) -> LuaResult<()> {
        self.scheduler.begin_dispatch();
        while let Some(event) = self.scheduler.next_active(delta) {
            // Deliberately leave the shared active cursor intact on error. A
            // nested AnimationWrapper call may be protected by Lua `pcall`,
            // in which case the outer scheduler resumes at the next item.
            self.dispatch_one(lua, event)?;
        }
        self.scheduler.finish_dispatch();
        Ok(())
    }

    pub(crate) fn discard_unvisited_after_host_error(&self) {
        for event in self.scheduler.take_unvisited_active() {
            match event {
                ApplicationEvent::Url => self.url_requests.discard_completion(),
                ApplicationEvent::GameServer => self.game_server.discard_completion(),
                ApplicationEvent::ServerTime => self.server_time.discard_completion(),
                ApplicationEvent::GamerServicesAuthentication => {
                    self.gamer_services.discard_authentication_completion()
                }
                ApplicationEvent::SocialLocal => self.social.discard_local_completion(),
                ApplicationEvent::SocialOnline => self.social.discard_online_completion(),
                ApplicationEvent::Assets => self.assets.discard_completion(),
                ApplicationEvent::ChannelLoadingFailure => {
                    self.channel.discard_loading_failure();
                }
                ApplicationEvent::ChannelContent => self.channel.discard_content_completion(),
                ApplicationEvent::Iap => self.iap.discard_completion(),
                ApplicationEvent::IapInitializationRetry(_) => {}
                ApplicationEvent::SdkLogFlush(_) => {}
                // Resumed owns no service payload/FIFO entry. Discarding an
                // unvisited event after a fatal host error must not run it.
                ApplicationEvent::ApplicationResumed => {}
                ApplicationEvent::SkynestSessionSuccess => {
                    self.skynest_account.pop_session_success();
                }
                ApplicationEvent::QrRecognition => self.qr_scanner.discard_completion(),
                ApplicationEvent::SkynestAccountLocal => {
                    self.skynest_account.discard_local_completion();
                }
                ApplicationEvent::SkynestAccountOnline => {
                    self.skynest_account.discard_online_completion();
                }
                ApplicationEvent::SkynestStorageLocal => {
                    self.skynest_storage.discard_local_completion();
                }
                ApplicationEvent::SkynestStorageOnline => {
                    self.skynest_storage.discard_online_completion();
                }
                // The token owns the identity of the entity to remove, not a
                // slot in a parallel FIFO. Dropping it cannot consume a later
                // deletion, and an embedding may inspect the retained scene
                // after the otherwise-fatal native host error.
                ApplicationEvent::AnimationEntityRemoval(_)
                | ApplicationEvent::AnimationEntityAttachment(_) => {}
            }
        }
    }
}

pub(crate) fn install_application_event_dispatcher(
    lua: &Lua,
    dispatcher: ApplicationEventDispatcher,
) -> LuaResult<()> {
    let attachment_scheduler = dispatcher.scheduler.clone();
    lua.set_named_registry_value(
        POST_ENTITY_ATTACHMENT_REGISTRY_KEY,
        lua.create_function(move |_, generation: u64| {
            attachment_scheduler.post(ApplicationEvent::AnimationEntityAttachment(generation));
            Ok(())
        })?,
    )?;
    let scheduler = dispatcher.scheduler.clone();
    lua.set_named_registry_value(
        POST_ENTITY_REMOVAL_REGISTRY_KEY,
        lua.create_function(move |_, generation: u64| {
            scheduler.post(ApplicationEvent::AnimationEntityRemoval(generation));
            Ok(())
        })?,
    )?;
    lua.set_named_registry_value(
        DISPATCH_REGISTRY_KEY,
        lua.create_function(move |lua, ()| dispatcher.dispatch(lua, 0.0))?,
    )
}

/// Entity::setParent queues attachment after the old entity's removal. The
/// payload is owned by the animation runtime until this exact token runs.
pub(crate) fn post_animation_entity_attachment(lua: &Lua, generation: u64) -> LuaResult<()> {
    match lua.named_registry_value::<Value>(POST_ENTITY_ATTACHMENT_REGISTRY_KEY)? {
        Value::Function(post) => post.call(generation),
        _ => lua
            .named_registry_value::<mlua::Function>(ANIMATION_ENTITY_ATTACHMENT_REGISTRY_KEY)?
            .call(generation),
    }
}

/// Entity::remove posts a retained entity identity through the same native
/// scheduler as platform callbacks (`sub_10043D464` → `sub_10057C1BC`).
/// Standalone animation fixtures have no host scheduler, so their empty
/// scheduler is represented by immediate execution of this sole event.
pub(crate) fn post_animation_entity_removal(lua: &Lua, generation: u64) -> LuaResult<()> {
    match lua.named_registry_value::<Value>(POST_ENTITY_REMOVAL_REGISTRY_KEY)? {
        Value::Function(post) => post.call(generation),
        _ => lua
            .named_registry_value::<mlua::Function>(ANIMATION_ENTITY_REMOVAL_REGISTRY_KEY)?
            .call(generation),
    }
}

/// Invoke a scheduler call site embedded inside another native subsystem.
/// Standalone AnimationWrapper unit tests intentionally have no application
/// dispatcher registered and therefore treat the hook as an empty queue.
pub(crate) fn dispatch_registered_application_events(lua: &Lua) -> LuaResult<()> {
    match lua.named_registry_value::<Value>(DISPATCH_REGISTRY_KEY)? {
        Value::Function(dispatch) => dispatch.call(()),
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn drain(scheduler: &ApplicationEventScheduler, delta: f32) -> Vec<ApplicationEvent> {
        scheduler.begin_dispatch();
        let mut result = Vec::new();
        while let Some(event) = scheduler.next_active(delta) {
            result.push(event);
        }
        scheduler.finish_dispatch();
        result
    }

    #[test]
    fn delayed_events_do_not_block_fifo_and_zero_delta_drains_do_not_age_them() {
        let scheduler = ApplicationEventScheduler::default();
        scheduler.post_delayed(ApplicationEvent::IapInitializationRetry(1), 10.0);
        scheduler.post(ApplicationEvent::Url);
        scheduler.post_delayed(ApplicationEvent::IapInitializationRetry(2), 2.0);
        assert_eq!(
            drain(&scheduler, 3.0),
            [
                ApplicationEvent::Url,
                ApplicationEvent::IapInitializationRetry(2)
            ]
        );
        assert!(drain(&scheduler, 0.0).is_empty());
        assert!(drain(&scheduler, 6.5).is_empty());
        assert_eq!(
            drain(&scheduler, 0.5),
            [ApplicationEvent::IapInitializationRetry(1)]
        );
        assert!(drain(&scheduler, 100.0).is_empty());
    }

    #[test]
    fn callback_post_is_not_aged_until_a_following_dispatch() {
        let scheduler = ApplicationEventScheduler::default();
        scheduler.post(ApplicationEvent::Url);
        scheduler.begin_dispatch();
        assert_eq!(scheduler.next_active(100.0), Some(ApplicationEvent::Url));
        scheduler.post_delayed(ApplicationEvent::IapInitializationRetry(1), 10.0);
        assert_eq!(scheduler.next_active(100.0), None);
        scheduler.finish_dispatch();
        assert!(drain(&scheduler, 9.0).is_empty());
        assert_eq!(
            drain(&scheduler, 1.0),
            [ApplicationEvent::IapInitializationRetry(1)]
        );
    }

    #[test]
    fn nested_drain_compacts_and_resets_shared_cursor_before_outer_walk_resumes() {
        let scheduler = ApplicationEventScheduler::default();
        scheduler.post_delayed(ApplicationEvent::IapInitializationRetry(1), 10.0);
        scheduler.post(ApplicationEvent::Url);
        scheduler.begin_dispatch();
        assert_eq!(scheduler.next_active(2.0), Some(ApplicationEvent::Url));
        scheduler.post(ApplicationEvent::GameServer);
        assert_eq!(drain(&scheduler, 0.0), [ApplicationEvent::GameServer]);
        // Native reloads qword_100C28938 after the callback. Nested finish
        // reset it to zero: the outer walk revisits the retained timer using
        // its own S0 (2), even though the nested walk itself used zero.
        assert_eq!(scheduler.next_active(2.0), None);
        scheduler.finish_dispatch();
        assert!(drain(&scheduler, 5.5).is_empty());
        assert_eq!(
            drain(&scheduler, 0.5),
            [ApplicationEvent::IapInitializationRetry(1)]
        );
    }

    #[test]
    fn cancellation_includes_visited_positive_timers_without_skipping_tail() {
        let scheduler = ApplicationEventScheduler::default();
        let timer = ApplicationEvent::IapInitializationRetry(1);
        scheduler.post_delayed(timer, 10.0);
        scheduler.post(ApplicationEvent::Url);
        scheduler.post(ApplicationEvent::GameServer);
        scheduler.begin_dispatch();
        assert_eq!(scheduler.next_active(1.0), Some(ApplicationEvent::Url));
        scheduler.cancel(timer);
        assert_eq!(
            scheduler.next_active(1.0),
            Some(ApplicationEvent::GameServer)
        );
        assert_eq!(scheduler.next_active(1.0), None);
        scheduler.finish_dispatch();
        assert!(drain(&scheduler, 100.0).is_empty());
    }

    #[test]
    fn host_error_retains_visited_unexpired_timer_and_drops_unvisited_payloads() {
        let scheduler = ApplicationEventScheduler::default();
        scheduler.post_delayed(ApplicationEvent::IapInitializationRetry(1), 10.0);
        scheduler.post(ApplicationEvent::Url);
        scheduler.post(ApplicationEvent::GameServer);
        scheduler.begin_dispatch();
        assert_eq!(scheduler.next_active(3.0), Some(ApplicationEvent::Url));
        scheduler.post(ApplicationEvent::ServerTime);
        assert_eq!(
            scheduler.take_unvisited_active(),
            [ApplicationEvent::GameServer]
        );
        assert_eq!(drain(&scheduler, 6.0), [ApplicationEvent::ServerTime]);
        assert_eq!(
            drain(&scheduler, 1.0),
            [ApplicationEvent::IapInitializationRetry(1)]
        );
    }

    #[test]
    fn missing_dispatch_registry_is_an_empty_queue() {
        dispatch_registered_application_events(&Lua::new()).unwrap();
    }

    #[test]
    fn cancelled_active_channel_request_cannot_consume_its_replacement() {
        for reenter in [false, true] {
            let runtime = StellaLua::new("/tmp").unwrap();
            runtime
                .lua()
                .set_named_registry_value("stella.rovio_channel.sdk_enabled", true)
                .unwrap();
            runtime.lua().globals().set("reenter", reenter).unwrap();
            runtime
                .execute_source(
                    r#"
                        application_event_order = {}
                        notifyEventManager = function() end
                        RovioChannel.onChannelLoadingFailed = function()
                            table.insert(application_event_order, "replacement")
                        end
                        local function open()
                            RovioChannel.openChannelView(
                                "Purple", "full", "en_EN", 1024, 768, "", "map_screen")
                        end
                        _G.SkynestStorage.native_setKey("outer", "value", function()
                            table.insert(application_event_order, "outer")
                            RovioChannel.cancelChannelViewLoading()
                            open()
                            if reenter then AnimationWrapperNative.update(0) end
                            table.insert(application_event_order, "returned")
                        end)
                        open()
                        _G.SkynestAccount.native_validateNickname("tail", function()
                            table.insert(application_event_order, "tail")
                        end)
                    "#,
                )
                .unwrap();
            runtime.update(0.0).unwrap();
            let order: mlua::Table = game_environment(runtime.lua())
                .unwrap()
                .get("application_event_order")
                .unwrap();
            let actual = order
                .sequence_values::<String>()
                .collect::<LuaResult<Vec<_>>>()
                .unwrap();
            if reenter {
                assert_eq!(actual, ["outer", "tail", "replacement", "returned"]);
            } else {
                assert_eq!(actual, ["outer", "returned", "tail"]);
                runtime.update(0.0).unwrap();
                let actual = order
                    .sequence_values::<String>()
                    .collect::<LuaResult<Vec<_>>>()
                    .unwrap();
                assert_eq!(actual, ["outer", "returned", "tail", "replacement"]);
            }
        }
    }

    #[test]
    fn update_body_scheduler_error_discards_old_active_payloads_but_keeps_new_posts() {
        let runtime = StellaLua::new("/tmp").unwrap();
        runtime
            .execute_source(
                r#"
            application_event_order = {}
            notifyEventManager = function() end
            update = function()
                update = nil
                _G.SkynestStorage.native_setKey("failure", "value", function()
                    _G.SkynestAccount.native_validateNickname("fresh", function()
                        table.insert(application_event_order, "fresh")
                    end)
                    error("body-scheduler-stop")
                end)
                _G.SkynestAccount.native_validateNickname("abandoned", function()
                    table.insert(application_event_order, "abandoned")
                end)
                AnimationWrapperNative.update(0)
            end
        "#,
            )
            .unwrap();
        let error = runtime.update(0.0).unwrap_err();
        assert!(error.to_string().contains("body-scheduler-stop"));
        runtime.update(0.0).unwrap();
        let order: mlua::Table = game_environment(runtime.lua())
            .unwrap()
            .get("application_event_order")
            .unwrap();
        assert_eq!(order.raw_len(), 1);
        assert_eq!(order.get::<String>(1).unwrap(), "fresh");
    }
}
