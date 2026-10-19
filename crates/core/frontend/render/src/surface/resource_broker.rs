use mesh_core_resources::{
    ResourceByteBudget, ResourceByteReservation, ResourcePreparationToken, resource_revision,
};
use std::sync::mpsc::{self, SyncSender};
use std::sync::{Arc, Mutex, OnceLock, RwLock};

use std::cell::RefCell;
use std::collections::HashMap;
use std::hash::Hash;
use std::sync::Weak;
use std::sync::atomic::{AtomicBool, Ordering};

thread_local! {
    static PAINT_CONSUMER: RefCell<Option<Weak<AtomicBool>>> = const { RefCell::new(None) };
}

/// Tracks completed resources requested while painting one component's targets.
/// Dropping the consumer releases its subscriptions without retaining the component.
#[derive(Default)]
pub struct ResourcePaintConsumer(Arc<AtomicBool>);

impl ResourcePaintConsumer {
    /// Attribute resource requests in this scope to this consumer. Nested scopes
    /// restore the previous consumer when their guard is dropped.
    pub fn enter(&self) -> ResourcePaintScope {
        ResourcePaintScope(
            PAINT_CONSUMER.with(|active| active.replace(Some(Arc::downgrade(&self.0)))),
            std::marker::PhantomData,
        )
    }

    pub fn is_ready(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }

    /// Consume the repaint request coalesced from this consumer's finished loads.
    pub fn take_ready(&self) -> bool {
        self.0.swap(false, Ordering::AcqRel)
    }
}

#[must_use = "keep the resource paint scope alive while painting"]
pub struct ResourcePaintScope(
    Option<Weak<AtomicBool>>,
    std::marker::PhantomData<std::rc::Rc<()>>,
);

impl Drop for ResourcePaintScope {
    fn drop(&mut self) {
        PAINT_CONSUMER.with(|active| active.replace(self.0.take()));
    }
}

#[derive(Default)]
pub(crate) struct ResourceWaiters(Vec<Weak<AtomicBool>>);

impl ResourceWaiters {
    fn subscribe_current(&mut self) {
        PAINT_CONSUMER.with(|active| {
            if let Some(consumer) = active.borrow().as_ref() {
                self.0.retain(|waiter| waiter.strong_count() > 0);
                if !self.0.iter().any(|waiter| waiter.ptr_eq(consumer)) {
                    self.0.push(consumer.clone());
                }
            }
        });
    }

    pub(crate) fn notify(self) {
        for consumer in self.0.into_iter().filter_map(|waiter| waiter.upgrade()) {
            consumer.store(true, Ordering::Release);
        }
    }
}

pub(crate) struct PendingResourceConsumers<K>(HashMap<K, ResourceWaiters>);

impl<K> Default for PendingResourceConsumers<K> {
    fn default() -> Self {
        Self(HashMap::new())
    }
}

impl<K: Eq + Hash> PendingResourceConsumers<K> {
    pub(crate) fn insert(&mut self, key: K) -> bool {
        let new = !self.0.contains_key(&key);
        self.0.entry(key).or_default().subscribe_current();
        new
    }

    pub(crate) fn remove(&mut self, key: &K) -> ResourceWaiters {
        self.0.remove(key).unwrap_or_default()
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

const RESOURCE_BROKER_QUEUE_CAPACITY: usize = 128;
const RESOURCE_BROKER_MAX_BYTES: usize = 64 * 1024 * 1024;

type ResourceWork = Box<dyn FnOnce(ResourceBrokerContext) + Send + 'static>;
type ResourceCancellation = Box<dyn FnOnce(ResourceByteReservation) + Send + 'static>;

type CompletionWaker = Arc<dyn Fn() + Send + Sync>;

static COMPLETION_WAKER: RwLock<Option<CompletionWaker>> = RwLock::new(None);

/// Install the callback the resource worker runs after each job settles,
/// whether it produced a result or was cancelled. The shell passes its loop
/// wake so it can sleep until a result is ready instead of polling. `None`
/// removes it.
pub fn set_resource_completion_waker(waker: Option<CompletionWaker>) {
    *COMPLETION_WAKER
        .write()
        .unwrap_or_else(|poisoned| poisoned.into_inner()) = waker;
}

/// Whether a completion waker is installed, so a scheduler knows that pending
/// resource work will wake it.
pub fn resource_completion_waker_installed() -> bool {
    COMPLETION_WAKER
        .read()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .is_some()
}

fn wake_after_completion() {
    let waker = COMPLETION_WAKER
        .read()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clone();
    if let Some(waker) = waker {
        waker();
    }
}

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
            wake_after_completion();
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
        wake_after_completion();
    }
}

#[cfg(test)]
mod tests {

    #[test]
    #[ignore = "release-only resource completion fanout profile"]
    fn resource_completion_fanout_profile() {
        let consumers: Vec<_> = (0..32)
            .map(|_| super::ResourcePaintConsumer::default())
            .collect();
        let mut requests = 0;
        let started = std::time::Instant::now();
        for _ in 0..1000 {
            let mut pending = super::PendingResourceConsumers::default();
            for resource in 0..8 {
                for consumer in &consumers[..2] {
                    let _scope = consumer.enter();
                    pending.insert(resource);
                }
            }
            for resource in 0..8 {
                pending.remove(&resource).notify();
            }
            requests += consumers
                .iter()
                .filter(|consumer| consumer.take_ready())
                .count();
        }
        eprintln!(
            "32 components, 8 shared loads/batch, 1000 batches: {requests} requesting-component repaints vs 32000 broadcast repaints; {:?}",
            started.elapsed()
        );
        assert_eq!(requests, 2000);
        println!(
            "MESH_PERF metric=resource_completion_repaint_fraction value={:.4}",
            requests as f64 / 32000.0
        );
    }
    #[test]
    fn resource_completions_notify_only_requesting_consumers() {
        let first = super::ResourcePaintConsumer::default();
        let second = super::ResourcePaintConsumer::default();
        let unrelated = super::ResourcePaintConsumer::default();
        let dropped = super::ResourcePaintConsumer::default();
        let mut pending = super::PendingResourceConsumers::default();
        {
            let _scope = first.enter();
            assert!(pending.insert("shared"));
            assert!(!pending.insert("shared"));
            {
                let _nested = second.enter();
                assert!(!pending.insert("shared"));
                assert!(pending.insert("second-only"));
            }
            assert!(pending.insert("first-only"));
        }
        {
            let _scope = dropped.enter();
            pending.insert("shared");
        }
        drop(dropped);
        assert_eq!(pending.0["shared"].0.len(), 3);
        pending.remove(&"shared").notify();
        assert!(first.take_ready());
        assert!(second.take_ready());
        assert!(!unrelated.take_ready());
        assert!(!first.take_ready());
        pending.remove(&"first-only").notify();
        assert!(first.take_ready());
        assert!(!second.take_ready());
        pending.remove(&"second-only").notify();
        assert!(second.take_ready());
        assert!(!first.take_ready());
    }

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
