use mesh_core_resources::{
    ResourceByteBudget, ResourceByteReservation, ResourcePreparationToken, resource_revision,
};
use std::sync::mpsc::{self, SyncSender};
use std::sync::{Arc, Mutex, OnceLock};

const RESOURCE_BROKER_QUEUE_CAPACITY: usize = 128;
const RESOURCE_BROKER_MAX_BYTES: usize = 32 * 1024 * 1024;

type ResourceWork = Box<dyn FnOnce(ResourceBrokerContext) + Send + 'static>;
type ResourceCancellation = Box<dyn FnOnce(ResourceByteReservation) + Send + 'static>;

struct ResourceJob {
    generation: u64,
    token: ResourcePreparationToken,
    reservation: ResourceByteReservation,
    work: ResourceWork,
    cancellation: ResourceCancellation,
}

struct ActiveGeneration {
    generation: u64,
    token: ResourcePreparationToken,
}

struct ResourceBrokerState {
    active: Mutex<ActiveGeneration>,
    budget: ResourceByteBudget,
}

/// Shared execution and admission boundary for derived render resources.
///
/// Icon images and font glyphs keep separate typed result queues because their
/// caches and handoff semantics differ, but all blocking preparation enters
/// this one bounded queue. The broker owns the generation token and byte
/// reservation until a worker hands a result back to its typed consumer.
#[derive(Clone)]
pub(crate) struct ResourceBroker {
    state: Arc<ResourceBrokerState>,
    sender: SyncSender<ResourceJob>,
}

pub(crate) struct ResourceBrokerContext {
    state: Arc<ResourceBrokerState>,
    generation: u64,
    token: ResourcePreparationToken,
    reservation: Option<ResourceByteReservation>,
    cancellation: Option<ResourceCancellation>,
}

impl ResourceBrokerContext {
    pub(crate) fn is_current(&self) -> bool {
        is_current_generation(&self.state, self.generation, &self.token)
    }

    pub(crate) fn into_reservation(mut self) -> ResourceByteReservation {
        self.cancellation.take();
        self.reservation
            .take()
            .expect("resource broker context reservation is available once")
    }
}

impl Drop for ResourceBrokerContext {
    fn drop(&mut self) {
        if let (Some(reservation), Some(cancellation)) =
            (self.reservation.take(), self.cancellation.take())
        {
            cancellation(reservation);
        }
    }
}

impl ResourceBroker {
    fn new() -> Option<Self> {
        let state = Arc::new(ResourceBrokerState {
            active: Mutex::new(ActiveGeneration {
                generation: resource_revision(),
                token: ResourcePreparationToken::new(),
            }),
            budget: ResourceByteBudget::new(RESOURCE_BROKER_MAX_BYTES),
        });
        let (sender, receiver) = mpsc::sync_channel(RESOURCE_BROKER_QUEUE_CAPACITY);
        let worker_state = Arc::clone(&state);
        std::thread::Builder::new()
            .name("mesh-render-resource".into())
            .spawn(move || resource_worker(worker_state, receiver))
            .ok()?;
        Some(Self { state, sender })
    }

    pub(crate) fn global() -> Option<&'static Self> {
        static BROKER: OnceLock<Option<ResourceBroker>> = OnceLock::new();
        BROKER.get_or_init(Self::new).as_ref()
    }

    pub(crate) fn submit(
        &self,
        bytes: usize,
        work: impl FnOnce(ResourceBrokerContext) + Send + 'static,
        cancellation: impl FnOnce(ResourceByteReservation) + Send + 'static,
    ) -> Option<bool> {
        let active = self.refresh_generation();
        let Some(reservation) = self.state.budget.try_reserve(bytes) else {
            return Some(false);
        };
        let job = ResourceJob {
            generation: active.generation,
            token: active.token,
            reservation,
            work: Box::new(work),
            cancellation: Box::new(cancellation),
        };
        match self.sender.try_send(job) {
            Ok(()) => Some(true),
            Err(mpsc::TrySendError::Full(_)) => Some(false),
            Err(mpsc::TrySendError::Disconnected(_)) => None,
        }
    }

    fn refresh_generation(&self) -> ActiveGeneration {
        let generation = resource_revision();
        let mut active = self
            .state
            .active
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if active.generation != generation {
            active.token.cancel();
            active.generation = generation;
            active.token = ResourcePreparationToken::new();
        }
        ActiveGeneration {
            generation: active.generation,
            token: active.token.clone(),
        }
    }
}

fn is_current_generation(
    state: &ResourceBrokerState,
    generation: u64,
    token: &ResourcePreparationToken,
) -> bool {
    let current_generation = resource_revision();
    let active = state
        .active
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if active.generation != current_generation {
        active.token.cancel();
        return false;
    }
    active.generation == generation && !token.is_cancelled()
}

fn resource_worker(state: Arc<ResourceBrokerState>, receiver: mpsc::Receiver<ResourceJob>) {
    while let Ok(job) = receiver.recv() {
        let ResourceJob {
            generation,
            token,
            reservation,
            work,
            cancellation,
        } = job;
        if !is_current_generation(&state, generation, &token) {
            cancellation(reservation);
            continue;
        }
        let context = ResourceBrokerContext {
            state: Arc::clone(&state),
            generation,
            token,
            reservation: Some(reservation),
            cancellation: Some(cancellation),
        };
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| work(context)));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn broker_shares_one_byte_budget_across_queued_work() {
        let broker = ResourceBroker::new().expect("test worker should start");
        let (started_sender, started_receiver) = mpsc::sync_channel(1);
        let (release_sender, release_receiver) = mpsc::sync_channel(1);
        let (finished_sender, finished_receiver) = mpsc::sync_channel(1);

        assert_eq!(
            broker.submit(
                RESOURCE_BROKER_MAX_BYTES,
                move |context| {
                    started_sender.send(()).unwrap();
                    release_receiver.recv().unwrap();
                    drop(context.into_reservation());
                    finished_sender.send(()).unwrap();
                },
                |_reservation| {},
            ),
            Some(true)
        );
        started_receiver
            .recv_timeout(Duration::from_secs(1))
            .expect("broker worker did not start");

        assert_eq!(
            broker.submit(1, |_context| {}, |_reservation| {}),
            Some(false),
            "icon and glyph work must draw from one shared budget"
        );

        release_sender.send(()).unwrap();
        finished_receiver
            .recv_timeout(Duration::from_secs(1))
            .expect("broker worker did not finish");
    }

    #[test]
    fn broker_turns_worker_panics_into_typed_cancellation_and_keeps_running() {
        let broker = ResourceBroker::new().expect("test worker should start");
        let (cancelled_sender, cancelled_receiver) = mpsc::sync_channel(1);
        assert_eq!(
            broker.submit(
                1,
                |_context| panic!("test render resource panic"),
                move |reservation| {
                    drop(reservation);
                    cancelled_sender.send(()).unwrap();
                },
            ),
            Some(true)
        );
        cancelled_receiver
            .recv_timeout(Duration::from_secs(1))
            .expect("worker panic did not invoke cancellation");

        let (finished_sender, finished_receiver) = mpsc::sync_channel(1);
        assert_eq!(
            broker.submit(
                1,
                move |context| {
                    drop(context.into_reservation());
                    finished_sender.send(()).unwrap();
                },
                |_reservation| panic!("healthy follow-up work was cancelled"),
            ),
            Some(true)
        );
        finished_receiver
            .recv_timeout(Duration::from_secs(1))
            .expect("broker worker did not recover after a panic");
    }
}
