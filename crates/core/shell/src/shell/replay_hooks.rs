//! Test-only seam that lets a harness drive the real [`Shell`] event loop
//! against a live compositor. The loop calls the driver once per iteration,
//! after Wayland dispatch, and never sleeps past the instant it asked for.

use super::Shell;
use std::time::Instant;

/// What the driver wants from the loop after one call.
pub(in crate::shell) struct ReplayStep {
    /// The loop wakes no later than this instant.
    pub(in crate::shell) wake_at: Option<Instant>,
}

pub(in crate::shell) struct ReplayHarness {
    /// Leave host-backed service providers unspawned so injected service
    /// state is the only source.
    pub(in crate::shell) suppress_backends: bool,
    pub(in crate::shell) driver: Box<dyn FnMut(&mut Shell) -> ReplayStep>,
    pub(in crate::shell) wake_at: Option<Instant>,
}

impl Shell {
    pub(in crate::shell) fn run_replay_driver(&mut self) {
        let Some(mut replay) = self.replay.take() else {
            return;
        };
        let step = (replay.driver)(self);
        replay.wake_at = step.wake_at;
        self.replay = Some(replay);
    }

    pub(in crate::shell) fn clamp_to_replay_wake(
        &self,
        deadline: std::time::Duration,
    ) -> std::time::Duration {
        match self.replay.as_ref().and_then(|replay| replay.wake_at) {
            Some(at) => deadline.min(at.saturating_duration_since(Instant::now())),
            None => deadline,
        }
    }
}
