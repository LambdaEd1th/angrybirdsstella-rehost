//! Retained session-success event (native symbol 0x100C2B95C).

use super::{IdentitySession, SessionState};
use crate::{ApplicationEvent, ApplicationEventScheduler};
use std::collections::VecDeque;

#[derive(Default)]
pub(super) struct SessionEvents {
    scheduler: Option<ApplicationEventScheduler>,
    owners: VecDeque<(u64, u64)>,
}

impl IdentitySession {
    pub(in super::super) fn bind_success_events(&self, scheduler: ApplicationEventScheduler) {
        self.sdk_logger.bind_scheduler(scheduler.clone());
        self.success_events
            .lock()
            .expect("session event lock poisoned")
            .scheduler = Some(scheduler);
    }

    pub(super) fn publish_success_locked(&self, state: &SessionState) {
        let mut events = self
            .success_events
            .lock()
            .expect("session event lock poisoned");
        let Some(scheduler) = events.scheduler.clone() else {
            return;
        };
        // Capture the publisher's owner under the publication lock; never
        // resample a potentially replaced account after releasing it. Native
        // posts this event before its retained request-success continuation.
        events
            .owners
            .push_back((state.epoch, state.identity_generation));
        scheduler.post(ApplicationEvent::SkynestSessionSuccess);
    }

    pub(in super::super) fn pop_success_owner(&self) -> Option<(u64, u64)> {
        self.success_events
            .lock()
            .expect("session event lock poisoned")
            .owners
            .pop_front()
    }
}
