//! Skynest device-log listener: 100680938/F60, 1006813F0/140/BD4.

use super::{
    IdentityConfig, REQUEST_TIMEOUT,
    session::{IdentitySession, PreparedRequest, ProviderLevel, RequestOwner},
};
use crate::{ApplicationEvent, ApplicationEventScheduler, SdkLogLevel, SdkLogSnapshot};
use base64::{Engine as _, engine::general_purpose::URL_SAFE};
use serde::Serialize;
use std::{
    sync::{Arc, Mutex},
    time::{Instant, SystemTime, UNIX_EPOCH},
};

#[derive(Clone)]
struct Binding {
    config: IdentityConfig,
    url: String,
}

#[derive(Serialize)]
struct Record {
    message: String,
    time: u64,
    tag: String,
    level: &'static str,
}

#[derive(Default)]
struct State {
    generation: u64,
    binding: Option<Binding>,
    scheduler: Option<ApplicationEventScheduler>,
    records: Vec<Record>,
    snapshot: SdkLogSnapshot,
}

struct Batch {
    generation: u64,
    binding: Binding,
    owner: RequestOwner,
    records: Vec<Record>,
}

enum UploadOutcome {
    Success,
    Cancelled,
    Failure(String),
}

#[derive(Default)]
pub(super) struct SdkLogger {
    state: Mutex<State>,
    send_gate: Mutex<()>,
    clock: Mutex<Option<(u64, Instant)>>,
}

impl SdkLogger {
    pub(super) fn bind_scheduler(&self, scheduler: ApplicationEventScheduler) {
        self.state
            .lock()
            .expect("SDK logger lock poisoned")
            .scheduler = Some(scheduler);
    }

    pub(super) fn configure(&self, config: &IdentityConfig, value: &str) {
        // Empty never constructs the singleton or changes its existing level.
        if value.is_empty() {
            return;
        }
        let mut state = self.state.lock().expect("SDK logger lock poisoned");
        if state.snapshot.fatal {
            return;
        }
        if state.binding.is_none() {
            let short_id = short_device_id(&config.identifiers.persistent_guid);
            let sessions = config.endpoint.session_url(&config.client_id);
            let app = sessions
                .strip_suffix("/sessions")
                .expect("native sessions path");
            state.binding = Some(Binding {
                config: config.clone(),
                url: format!("{app}/test_devices/{short_id}/logs"),
            });
        }
        state.snapshot.threshold = SdkLogLevel::from_config(value);
        // Setting OFF on an active listener changes filtering, but does not
        // unregister it, discard old records, or cancel the periodic callback.
        if !state.snapshot.listening && state.snapshot.threshold != SdkLogLevel::Off {
            state.snapshot.listening = true;
            schedule(&state);
        }
    }

    pub(super) fn reset_provider(&self) {
        // Explicit host endpoint/client replacement, not native logout. Stale
        // workers/timers must not transport data to a replacement provider.
        let mut state = self.state.lock().expect("SDK logger lock poisoned");
        let generation = state.generation.wrapping_add(1);
        let scheduler = state.scheduler.clone();
        // A completed fatal error is terminal for this host. Only a worker
        // whose generation was retired before completion may be ignored.
        let fatal = state.snapshot.fatal;
        let last_error = state.snapshot.last_error.clone().filter(|_| fatal);
        *state = State {
            generation,
            scheduler,
            snapshot: SdkLogSnapshot {
                fatal,
                last_error,
                ..SdkLogSnapshot::default()
            },
            ..State::default()
        };
    }

    pub(super) fn snapshot(&self) -> SdkLogSnapshot {
        let state = self.state.lock().expect("SDK logger lock poisoned");
        let mut result = state.snapshot.clone();
        result.queued_records = state.records.len();
        result
    }

    fn timestamp(&self) -> u64 {
        let mut clock = self.clock.lock().expect("SDK logger clock lock poisoned");
        // 100581134 anchors whole epoch seconds to a monotonic millisecond
        // clock once; subsequent wall-clock adjustments do not move records.
        let (epoch, start) = clock.get_or_insert_with(|| {
            (
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs()
                    .saturating_mul(1000),
                Instant::now(),
            )
        });
        epoch.saturating_add(start.elapsed().as_millis().try_into().unwrap_or(u64::MAX))
    }

    pub(super) fn submit(
        self: &Arc<Self>,
        session: &IdentitySession,
        level: SdkLogLevel,
        tag: &str,
        message: &str,
    ) -> bool {
        self.submit_at(session, level, tag, message, self.timestamp())
    }

    fn submit_at(
        self: &Arc<Self>,
        session: &IdentitySession,
        level: SdkLogLevel,
        tag: &str,
        message: &str,
        time: u64,
    ) -> bool {
        // Consistent lock order: session publication -> listener. Never obtain
        // a session owner while holding the listener's lock.
        let owner = session.request_owner(ProviderLevel::Level2);
        let batch = {
            let mut state = self.state.lock().expect("SDK logger lock poisoned");
            if state.snapshot.fatal
                || !state.snapshot.listening
                || level as i32 > state.snapshot.threshold as i32
            {
                return false;
            }
            state.records.push(Record {
                message: message.to_owned(),
                time,
                tag: tag.to_owned(),
                level: level.as_str(),
            });
            if state.records.len() >= 10 {
                take_batch(&mut state, owner)
            } else {
                None
            }
        };
        if let Some(batch) = batch {
            self.start_batch(session.clone(), batch);
        }
        true
    }

    pub(super) fn flush_timer(self: &Arc<Self>, session: &IdentitySession, generation: u64) {
        let owner = session.request_owner(ProviderLevel::Level2);
        let batch = {
            let mut state = self.state.lock().expect("SDK logger lock poisoned");
            if state.generation != generation || state.snapshot.fatal {
                return;
            }
            let batch = take_batch(&mut state, owner);
            if state.snapshot.listening {
                schedule(&state);
            }
            batch
        };
        if let Some(batch) = batch {
            self.start_batch(session.clone(), batch);
        }
    }

    fn is_current(&self, generation: u64) -> bool {
        let state = self.state.lock().expect("SDK logger lock poisoned");
        state.generation == generation && !state.snapshot.fatal
    }

    fn start_batch(self: &Arc<Self>, session: IdentitySession, batch: Batch) {
        let generation = batch.generation;
        let logger = Arc::clone(self);
        if let Err(error) = std::thread::Builder::new()
            .name("stella-sdk-logs".to_owned())
            .spawn(move || {
                logger.send_batch(&session, batch);
            })
        {
            self.finish_batch(
                generation,
                UploadOutcome::Failure(format!("SDK log worker start failed: {error}")),
            );
        }
    }

    fn send_batch(&self, session: &IdentitySession, mut batch: Batch) {
        // Native swaps the entire queue before spawning; failed requests do
        // not reinsert that batch. Its independent send mutex serializes HTTP.
        let _gate = self
            .send_gate
            .lock()
            .expect("SDK logger send lock poisoned");
        let body = serde_json::json!({"logs": batch.records}).to_string();
        let current = || self.is_current(batch.generation);
        let request = PreparedRequest {
            url: &batch.binding.url,
            body: Some(("application/json", body.as_bytes())),
            timeout: REQUEST_TIMEOUT,
            still_current: Some(&current),
        };
        let result = match session.execute_logger(&batch.binding.config, &mut batch.owner, &request)
        {
            Ok(_) => UploadOutcome::Success,
            // Explicit host ownership retirement is cancellation, not a
            // provider failure. The native thread also has a separate silent
            // ThreadInterruptedException exit (1005868A0..AC).
            Err(error) if error.is_stale() => UploadOutcome::Cancelled,
            Err(error) => UploadOutcome::Failure(error.to_string()),
        };
        // Common HTTP wrapper throws on non2xx, bypassing 100681E38..4C's
        // nominal deregistration. executeThread's std::exception/catch-all
        // branches then terminate (100586974/A14). Contain at host boundary.
        self.finish_batch(batch.generation, result);
    }

    fn finish_batch(&self, generation: u64, result: UploadOutcome) {
        let mut state = self.state.lock().expect("SDK logger lock poisoned");
        if state.generation != generation {
            return;
        }
        state.snapshot.in_flight_batches -= 1;
        match result {
            UploadOutcome::Success => state.snapshot.completed_batches += 1,
            UploadOutcome::Cancelled => state.snapshot.cancelled_batches += 1,
            UploadOutcome::Failure(error) => {
                eprintln!("SDK log upload failed: {error}");
                state.snapshot.failed_batches += 1;
                if !state.snapshot.fatal {
                    state.snapshot.last_error = Some(error);
                    state.snapshot.fatal = true;
                }
            }
        }
    }
}

fn take_batch(state: &mut State, owner: RequestOwner) -> Option<Batch> {
    if state.records.is_empty() {
        return None;
    }
    let binding = state.binding.clone()?;
    state.snapshot.in_flight_batches += 1;
    Some(Batch {
        generation: state.generation,
        binding,
        owner,
        records: std::mem::take(&mut state.records),
    })
}

fn schedule(state: &State) {
    if let Some(scheduler) = &state.scheduler {
        scheduler.post_delayed(ApplicationEvent::SdkLogFlush(state.generation), 5.0);
    }
}

fn short_device_id(device_guid: &str) -> String {
    // 100661494 ->1006612AC: CRC32, four native ARM64 little-endian bytes,
    // URL-safe padded BaseN (100559A48), then the first six characters.
    let mut crc = !0u32;
    for byte in device_guid.bytes() {
        crc ^= u32::from(byte);
        for _ in 0..8 {
            crc = (crc >> 1) ^ (0xedb8_8320 & 0u32.wrapping_sub(crc & 1));
        }
    }
    URL_SAFE.encode((!crc).to_le_bytes())[..6].to_owned()
}

#[cfg(test)]
mod tests;
