//! Serialized native GameClient, separate from opt-in local/JSON providers.
use super::super::skynest_account::game_support::GameIdentity;
use super::*;
use std::sync::mpsc::{self, Sender};

pub(in crate::game_lua::platform_services) mod protocol;
use protocol::{Cache, LeaderboardRow, PendingScore, Score};

#[derive(Debug)]
struct Lifetime {
    active: bool,
    persist_on_exit: bool,
}

struct Snapshot {
    identity: GameIdentity,
    text: String,
}

pub(super) struct GameHandle {
    tasks: Sender<Task>,
    lifetime: Arc<Mutex<Lifetime>>,
    identity: GameIdentity,
    counter: Arc<Mutex<i64>>,
    nocache: Arc<Mutex<bool>>,
    snapshot: Arc<Mutex<Snapshot>>,
}

impl std::fmt::Debug for GameHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("GameHandle")
    }
}

impl Drop for GameHandle {
    fn drop(&mut self) {
        let mut lifetime = self.lifetime.lock().expect("game lifetime lock poisoned");
        lifetime.active = false;
        if lifetime.persist_on_exit {
            let snapshot = self
                .snapshot
                .lock()
                .expect("game cache snapshot lock poisoned");
            let mut value: serde_json::Value =
                serde_json::from_str(&snapshot.text).expect("native cache snapshot is JSON");
            value["transactionId"] =
                (*self.counter.lock().expect("game counter lock poisoned")).into();
            let previous = &snapshot.identity;
            let current = previous
                .for_request()
                .filter(|current| previous.same_account(current));
            if let Some(current) = current
                && let Err(error) = current.store_cache(&protocol::compact(&value))
            {
                eprintln!("native game cache write failed: {error}");
            }
        }
        // Closing the last sender retires the worker. Native 10069496C saves
        // even an empty queue after stopping its executor. Explicit provider
        // retirement disables that final write before removing the handle.
    }
}

enum Task {
    Post {
        identity: GameIdentity,
        transaction_id: i64,
        score: Score,
        request_id: String,
    },
    Leaderboard {
        identity: GameIdentity,
        level: String,
        request_id: String,
    },
}

#[derive(Clone, Debug)]
pub(super) struct GameCompletion {
    identity: GameIdentity,
    lifetime: Arc<Mutex<Lifetime>>,
    result: GameResult,
}

#[derive(Clone, Debug)]
enum GameResult {
    Post {
        level: String,
        request_id: String,
        success: bool,
    },
    Leaderboard {
        level: String,
        request_id: String,
        rows: Option<Vec<LeaderboardRow>>,
    },
}

struct Pending {
    record: PendingScore,
    callback: Option<(String, String)>,
}

struct Worker {
    identity: GameIdentity,
    lifetime: Arc<Mutex<Lifetime>>,
    counter: Arc<Mutex<i64>>,
    nocache: Arc<Mutex<bool>>,
    cache: Cache,
    pending: Vec<Pending>,
    generation: u64,
    queue: Arc<Mutex<VecDeque<(u64, OnlineCompletion)>>>,
    events: ApplicationEventScheduler,
    snapshot: Arc<Mutex<Snapshot>>,
}

impl GameHandle {
    pub(super) fn new(
        runtime: &SocialRuntime,
        identity: GameIdentity,
        generation: u64,
    ) -> LuaResult<Self> {
        let mut cache = Cache::load(&identity.load_cache().map_err(runtime_error)?);
        let counter = Arc::new(Mutex::new(cache.transaction_id));
        let nocache = Arc::new(Mutex::new(false));
        let lifetime = Arc::new(Mutex::new(Lifetime {
            active: true,
            persist_on_exit: true,
        }));
        let snapshot = Arc::new(Mutex::new(Snapshot {
            identity: identity.clone(),
            text: cache.save(cache.transaction_id, &cache.pending),
        }));
        let pending = std::mem::take(&mut cache.pending)
            .into_iter()
            .map(|record| Pending {
                record,
                callback: None,
            })
            .collect();
        let mut worker = Worker {
            identity: identity.clone(),
            lifetime: lifetime.clone(),
            counter: counter.clone(),
            nocache: nocache.clone(),
            cache,
            pending,
            generation,
            queue: runtime.online_completions.clone(),
            events: runtime.application_events.clone(),
            snapshot: snapshot.clone(),
        };
        let (tasks, receiver) = mpsc::channel();
        std::thread::Builder::new()
            .name("stella-native-game".to_owned())
            .spawn(move || {
                while let Ok(task) = receiver.recv() {
                    if worker
                        .lifetime
                        .lock()
                        .expect("game lifetime lock poisoned")
                        .active
                    {
                        worker.run(task);
                    }
                }
            })
            .map_err(|_| runtime_error("Creating thread failed"))?;
        Ok(Self {
            tasks,
            lifetime,
            identity,
            counter,
            nocache,
            snapshot,
        })
    }

    pub(super) fn context_is_current(&self) -> bool {
        self.identity.context_is_current()
    }

    pub(super) fn retire(&self) {
        let mut lifetime = self.lifetime.lock().expect("game lifetime lock poisoned");
        lifetime.active = false;
        lifetime.persist_on_exit = false;
    }

    pub(super) fn social_connected(&self) {
        *self.nocache.lock().expect("game nocache lock poisoned") = true;
    }

    fn next_transaction(&self) -> i64 {
        let mut counter = self.counter.lock().expect("game counter lock poisoned");
        *counter = counter.wrapping_add(1);
        *counter
    }

    pub(super) fn post(&self, level: String, points: f32, request_id: String) -> LuaResult<()> {
        let Some(identity) = self.identity.for_request() else {
            return Ok(());
        };
        let transaction_id = self.next_transaction();
        self.tasks
            .send(Task::Post {
                identity,
                transaction_id,
                score: Score::new(level, points),
                request_id,
            })
            .map_err(|_| runtime_error("native game executor unavailable"))
    }

    pub(super) fn fetch(&self, level: String, request_id: String) -> LuaResult<()> {
        let Some(identity) = self.identity.for_request() else {
            return Ok(());
        };
        self.next_transaction();
        self.tasks
            .send(Task::Leaderboard {
                identity,
                level,
                request_id,
            })
            .map_err(|_| runtime_error("native game executor unavailable"))
    }
}

impl Worker {
    fn active(&self) -> bool {
        self.lifetime
            .lock()
            .expect("game lifetime lock poisoned")
            .active
            && self.identity.is_current()
    }

    fn publish(&self, result: GameResult) {
        let lifetime = self.lifetime.lock().expect("game lifetime lock poisoned");
        if !lifetime.active || !self.identity.is_current() {
            return;
        }
        self.queue
            .lock()
            .expect("social online completion lock poisoned")
            .push_back((
                self.generation,
                OnlineCompletion::NativeGame(Box::new(GameCompletion {
                    identity: self.identity.clone(),
                    lifetime: self.lifetime.clone(),
                    result,
                })),
            ));
        self.events.post(ApplicationEvent::SocialOnline);
    }

    fn update_snapshot(&self) -> String {
        let records: Vec<_> = self.pending.iter().map(|p| p.record.clone()).collect();
        let counter = *self.counter.lock().expect("game counter lock poisoned");
        let text = self.cache.save(counter, &records);
        *self
            .snapshot
            .lock()
            .expect("game cache snapshot lock poisoned") = Snapshot {
            identity: self.identity.clone(),
            text: text.clone(),
        };
        text
    }

    fn persist(&self) {
        let text = self.update_snapshot();
        if let Err(error) = self.identity.store_cache(&text) {
            eprintln!("native game cache write failed: {error}");
        }
    }

    fn flush(&mut self) {
        if self.pending.is_empty() {
            return;
        }
        // Complete compact roots are concatenated directly, without a newline
        // or array wrapper. No per-level max, sorting or coalescing occurs.
        let plaintext: String = self
            .pending
            .iter()
            .map(|p| p.record.score.submission())
            .collect();
        let lifetime = self.lifetime.clone();
        let current = || lifetime.lock().expect("game lifetime lock poisoned").active;
        let result = self.identity.post_scores(&plaintext, &current);
        if !self.active() {
            return;
        }
        let success = result.is_ok();
        let callbacks: Vec<_> = self
            .pending
            .iter_mut()
            .filter_map(|p| p.callback.take())
            .collect();
        for (level, request_id) in callbacks {
            self.publish(GameResult::Post {
                level,
                request_id,
                success,
            });
        }
        if success || result.is_err_and(|error| (400..500).contains(&error.status)) {
            self.pending.clear();
        }
        let lifetime = self.lifetime.lock().expect("game lifetime lock poisoned");
        if lifetime.active {
            self.persist();
        }
    }

    fn run(&mut self, task: Task) {
        let identity = match &task {
            Task::Post { identity, .. } | Task::Leaderboard { identity, .. } => identity.clone(),
        };
        if !identity.is_current() {
            return;
        }
        if !self.identity.same_account(&identity) {
            // Host ownership boundary: an account replacement cannot replay
            // the previous user's anonymous scores or cached records.
            let mut cache = Cache::load(&identity.load_cache().unwrap_or_default());
            self.pending = std::mem::take(&mut cache.pending)
                .into_iter()
                .map(|record| Pending {
                    record,
                    callback: None,
                })
                .collect();
            self.cache = cache;
        } else if !self.identity.same_request(&identity) {
            // A retained score can be retried, but a superseded request's Lua
            // callback must not be delivered under another profile generation.
            for pending in &mut self.pending {
                pending.callback = None;
            }
        }
        self.identity = identity;
        {
            let lifetime = self.lifetime.lock().expect("game lifetime lock poisoned");
            if !lifetime.active {
                return;
            }
            self.update_snapshot();
        }
        match task {
            Task::Post {
                identity: _,
                transaction_id,
                score,
                request_id,
            } => {
                if score.level.is_empty() || score.points < 0 {
                    self.publish(GameResult::Post {
                        level: score.level,
                        request_id,
                        success: false,
                    });
                    return;
                }
                self.pending.push(Pending {
                    callback: Some((score.level.clone(), request_id)),
                    record: PendingScore {
                        transaction_id,
                        score,
                    },
                });
                {
                    let lifetime = self.lifetime.lock().expect("game lifetime lock poisoned");
                    if !lifetime.active {
                        return;
                    }
                    self.update_snapshot();
                }
                self.flush();
            }
            Task::Leaderboard {
                identity: _,
                level,
                request_id,
            } => {
                // 10069A37C flushes pending submissions before validating the
                // dimension and before constructing the friends query.
                self.flush();
                if !self.active() {
                    return;
                }
                let mut rows = None;
                if !level.is_empty() {
                    let nocache = *self.nocache.lock().expect("game nocache lock poisoned");
                    let lifetime = self.lifetime.clone();
                    let current = || lifetime.lock().expect("game lifetime lock poisoned").active;
                    if let Ok(mut response) = self.identity.leaderboard(&level, nocache, &current) {
                        if response.status() != 200 {
                            *self.nocache.lock().expect("game nocache lock poisoned") = false;
                        } else {
                            const MAX: u64 = 8 * 1024 * 1024;
                            let mut bytes = Vec::new();
                            if response
                                .body_mut()
                                .as_reader()
                                .take(MAX + 1)
                                .read_to_end(&mut bytes)
                                .is_ok()
                                && bytes.len() as u64 <= MAX
                                && let Ok(text) = String::from_utf8(bytes)
                                && let Ok(parsed) = protocol::leaderboard(&text)
                            {
                                rows = Some(parsed);
                                *self.nocache.lock().expect("game nocache lock poisoned") = false;
                            }
                        }
                    }
                }
                self.publish(GameResult::Leaderboard {
                    level,
                    request_id,
                    rows,
                });
            }
        }
    }
}

pub(super) fn finish(
    lua: &Lua,
    runtime: &SocialRuntime,
    completion: GameCompletion,
) -> LuaResult<()> {
    if !completion
        .lifetime
        .lock()
        .expect("game lifetime lock poisoned")
        .active
        || !completion.identity.is_current()
    {
        return Ok(());
    }
    let native = lua.globals().get::<mlua::Table>("SocialManager")?;
    match completion.result {
        GameResult::Post {
            level,
            request_id,
            success,
        } => native
            .get::<mlua::Function>("onScorePosted")?
            .call::<()>((success, level, request_id)),
        GameResult::Leaderboard {
            level,
            request_id: _,
            rows: None,
        } => {
            // 1000C4848 retains only level; the failure has exactly two args.
            native
                .get::<mlua::Function>("onLeaderboardFetched")?
                .call::<()>((false, level))
        }
        GameResult::Leaderboard {
            level,
            request_id,
            rows: Some(rows),
        } => {
            let (own_id, own_name) = completion.identity.local_player().unwrap_or_default();
            let state = runtime
                .state
                .lock()
                .map_err(|_| runtime_error("social state lock poisoned"))?;
            let entries = lua.create_table()?;
            for (i, row) in rows.into_iter().enumerate() {
                let player = lua.create_table()?;
                player.set("accountId", row.account_id.clone())?;
                player.set("points", row.points as f32)?;
                player.set("rank", row.rank as f32)?;
                if row.account_id == own_id {
                    player.set("nickname", own_name.as_str())?;
                    player.set("localPlayer", true)?;
                } else {
                    let name = state
                        .friends_store
                        .as_ref()
                        .and_then(|store| store.friends.get(&row.account_id))
                        .map(LocalSocialFriend::display_name)
                        .unwrap_or("n/a");
                    player.set("nickname", name)?;
                }
                entries.raw_set(i + 1, player)?;
            }
            drop(state);
            native
                .get::<mlua::Function>("onLeaderboardFetched")?
                .call::<()>((true, level, entries, request_id))
        }
    }
}
