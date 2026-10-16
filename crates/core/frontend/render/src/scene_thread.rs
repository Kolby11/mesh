//! Immutable scene submissions and a bounded render-thread handoff.
//!
//! A renderer is constructed, used, and destroyed on its worker thread. The
//! UI sends owned plans and resource snapshots; it receives only transferable
//! presentation outputs. This seam does not enable a particular GPU backend.

use crate::{FractionalScale, FramePaintPlan};
use std::collections::{BTreeMap, VecDeque};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::{self, JoinHandle};

/// Resource inputs pinned for a submitted scene. Implementations must expose
/// immutable data and keep every referenced asset alive for the snapshot's life.
pub trait SceneResources: Send + Sync + 'static {
    fn revision(&self) -> u64;
}

/// A surface incarnation; a recreated surface must use a new incarnation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SceneSurface {
    pub id: u64,
    pub incarnation: u64,
}

/// Owned inputs for one surface frame, independent of a live widget tree or VM.
pub struct SceneSnapshot<R: SceneResources> {
    surface: SceneSurface,
    plan: Arc<FramePaintPlan>,
    resources: Arc<R>,
    scale: FractionalScale,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SceneSubmitError {
    ResourceRevisionMismatch,
    MissingSurfaceExtent,
    Closed,
    SurfaceRemoved,
    SurfaceCapacity,
    CommitExhausted,
}

impl<R: SceneResources> SceneSnapshot<R> {
    /// Pin a complete plan and matching resources. Plans without a surface
    /// extent or whose resource revision differs are rejected before submission.
    pub fn new(
        surface: SceneSurface,
        plan: Arc<FramePaintPlan>,
        resources: Arc<R>,
        scale: FractionalScale,
    ) -> Result<Self, SceneSubmitError> {
        if plan.inputs.resource_revision != resources.revision() {
            return Err(SceneSubmitError::ResourceRevisionMismatch);
        }
        if plan.inputs.surface_size.is_none() {
            return Err(SceneSubmitError::MissingSurfaceExtent);
        }
        Ok(Self {
            surface,
            plan,
            resources,
            scale,
        })
    }

    pub fn surface(&self) -> SceneSurface {
        self.surface
    }
    pub fn plan(&self) -> &FramePaintPlan {
        &self.plan
    }
    pub fn resources(&self) -> &R {
        &self.resources
    }
    pub fn scale(&self) -> FractionalScale {
        self.scale
    }
}

/// A submitted snapshot with a monotonically increasing commit identity.
pub struct SceneFrame<R: SceneResources> {
    snapshot: SceneSnapshot<R>,
    commit: u64,
    full_replay: bool,
}

impl<R: SceneResources> SceneFrame<R> {
    pub fn snapshot(&self) -> &SceneSnapshot<R> {
        &self.snapshot
    }
    pub fn commit(&self) -> u64 {
        self.commit
    }
    /// Coalesced or superseded frames can lose damage relative to the displayed
    /// buffer. The renderer must replay the complete topology and full surface
    /// when this is true, rather than using the plan's partial replay selection.
    pub fn requires_full_replay(&self) -> bool {
        self.full_replay
    }
}

/// Backend state stays on the worker thread and need not implement `Send`.
/// Output must own or lease its presentation resources independently of later
/// renders. Dropping an output must be safe on the UI thread or after renderer
/// shutdown; GPU contexts and mutable textures remain renderer-owned.
pub trait SceneRenderer: 'static {
    type Resources: SceneResources;
    type Output: Send + 'static;
    type Error: Send + 'static;
    fn render(&mut self, frame: &SceneFrame<Self::Resources>) -> Result<Self::Output, Self::Error>;
    fn remove_surface(&mut self, surface: SceneSurface);
}

pub struct RenderedScene<O, E> {
    pub surface: SceneSurface,
    pub commit: u64,
    pub output: Result<O, E>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SceneThreadMetrics {
    pub submitted: u64,
    pub coalesced: u64,
    pub rendered: u64,
    pub discarded: u64,
    pub pending: usize,
    pub completed: usize,
    pub in_flight: usize,
    pub surfaces: usize,
}

struct SurfaceState<R: SceneResources, O, E> {
    latest: u64,
    presented: u64,
    delivered: u64,
    identity: (u64, FractionalScale, (u32, u32)),
    pending: Option<SceneFrame<R>>,
    completed: Option<RenderedScene<O, E>>,
    queued: bool,
    in_flight: bool,
    removed: bool,
}

struct State<R: SceneResources, O, E> {
    surfaces: BTreeMap<SceneSurface, SurfaceState<R, O, E>>,
    ready: VecDeque<SceneSurface>,
    next_commit: u64,
    closed: bool,
    metrics: SceneThreadMetrics,
}

struct Shared<R: SceneResources, O, E> {
    state: Mutex<State<R, O, E>>,
    changed: Condvar,
    capacity: usize,
    wake_ui: Arc<dyn Fn() + Send + Sync>,
}

#[derive(Debug)]
pub enum SceneThreadStartError<E> {
    InvalidCapacity,
    Spawn(std::io::Error),
    Initialize(E),
    Panicked,
}

/// Bounded latest-frame queue: at most one pending frame, one active render,
/// and one completed output per admitted surface. Removed surfaces occupy
/// their slot until the renderer acknowledges resource retirement.
pub struct SceneRenderThread<B: SceneRenderer> {
    shared: Arc<Shared<B::Resources, B::Output, B::Error>>,
    worker: Option<JoinHandle<()>>,
}

impl<B: SceneRenderer> SceneRenderThread<B> {
    /// Construct the backend on its worker and wait for initialization. No
    /// rendering happens until a snapshot is submitted; idle workers block.
    pub fn spawn(
        capacity: usize,
        factory: impl FnOnce() -> Result<B, B::Error> + Send + 'static,
        wake_ui: impl Fn() + Send + Sync + 'static,
    ) -> Result<Self, SceneThreadStartError<B::Error>> {
        if capacity == 0 {
            return Err(SceneThreadStartError::InvalidCapacity);
        }
        let shared = Arc::new(Shared {
            state: Mutex::new(State {
                surfaces: BTreeMap::new(),
                ready: VecDeque::new(),
                next_commit: 0,
                closed: false,
                metrics: SceneThreadMetrics::default(),
            }),
            changed: Condvar::new(),
            capacity,
            wake_ui: Arc::new(wake_ui),
        });
        let work = Arc::clone(&shared);
        let (started, startup) = std::sync::mpsc::sync_channel(1);
        let worker = thread::Builder::new()
            .name("mesh-scene-render".into())
            .spawn(move || {
                struct WorkerExit<R: SceneResources, O, E>(Arc<Shared<R, O, E>>);
                impl<R: SceneResources, O, E> Drop for WorkerExit<R, O, E> {
                    fn drop(&mut self) {
                        let mut state = self
                            .0
                            .state
                            .lock()
                            .unwrap_or_else(std::sync::PoisonError::into_inner);
                        state.closed = true;
                        state.ready.clear();
                        let discarded: Vec<_> = state
                            .surfaces
                            .values_mut()
                            .map(|surface| (surface.pending.take(), surface.completed.take()))
                            .collect();
                        drop(state);
                        drop(discarded);
                        self.0.changed.notify_all();
                    }
                }
                let _exit = WorkerExit(Arc::clone(&work));
                let mut backend = match factory() {
                    Ok(backend) => backend,
                    Err(error) => {
                        let _ = started.send(Err(error));
                        return;
                    }
                };
                if started.send(Ok(())).is_err() {
                    return;
                }
                run_worker(&work, &mut backend);
            })
            .map_err(SceneThreadStartError::Spawn)?;
        match startup.recv() {
            Ok(Ok(())) => Ok(Self {
                shared,
                worker: Some(worker),
            }),
            Ok(Err(error)) => {
                let _ = worker.join();
                Err(SceneThreadStartError::Initialize(error))
            }
            Err(_) => {
                let _ = worker.join();
                Err(SceneThreadStartError::Panicked)
            }
        }
    }

    pub fn submit(&self, snapshot: SceneSnapshot<B::Resources>) -> Result<u64, SceneSubmitError> {
        let mut state = self.shared.state.lock().unwrap();
        if state.closed {
            return Err(SceneSubmitError::Closed);
        }
        let key = snapshot.surface;
        if !state.surfaces.contains_key(&key) && state.surfaces.len() >= self.shared.capacity {
            return Err(SceneSubmitError::SurfaceCapacity);
        }
        if state
            .surfaces
            .get(&key)
            .is_some_and(|surface| surface.removed)
        {
            return Err(SceneSubmitError::SurfaceRemoved);
        }
        let commit = state
            .next_commit
            .checked_add(1)
            .ok_or(SceneSubmitError::CommitExhausted)?;
        state.next_commit = commit;
        let identity = (
            snapshot.plan.inputs.resource_revision,
            snapshot.scale,
            snapshot
                .plan
                .inputs
                .surface_size
                .expect("validated snapshot extent"),
        );
        let entry = state.surfaces.entry(key).or_insert(SurfaceState {
            latest: commit,
            presented: 0,
            delivered: 0,
            identity,
            pending: None,
            completed: None,
            queued: false,
            in_flight: false,
            removed: false,
        });
        let full_replay = entry.latest != entry.presented || entry.identity != identity;
        entry.identity = identity;
        let coalesced = entry.pending.is_some();
        entry.latest = commit;
        let discarded_frame = entry.pending.replace(SceneFrame {
            snapshot,
            commit,
            full_replay,
        });
        let discarded_output = entry.completed.take();
        if !entry.queued {
            entry.queued = true;
            state.ready.push_back(key);
        }
        state.metrics.submitted += 1;
        state.metrics.coalesced += u64::from(coalesced);
        drop(state);
        drop(discarded_frame);
        drop(discarded_output);
        self.shared.changed.notify_one();
        Ok(commit)
    }

    /// Cancel pending presentation and retire this incarnation on the render
    /// thread after any active render finishes. Repeated removal is idempotent.
    pub fn remove_surface(&self, key: SceneSurface) -> bool {
        let mut state = self.shared.state.lock().unwrap();
        let Some(surface) = state.surfaces.get_mut(&key) else {
            return false;
        };
        if surface.removed {
            return false;
        }
        surface.removed = true;
        let discarded_frame = surface.pending.take();
        let discarded_output = surface.completed.take();
        if !surface.queued {
            surface.queued = true;
            state.ready.push_back(key);
        }
        drop(state);
        drop(discarded_frame);
        drop(discarded_output);
        self.shared.changed.notify_one();
        true
    }

    pub fn take_completed(&self, key: SceneSurface) -> Option<RenderedScene<B::Output, B::Error>> {
        let mut state = self.shared.state.lock().unwrap();
        let surface = state.surfaces.get_mut(&key)?;
        let completed = surface.completed.take()?;
        surface.delivered = if completed.output.is_ok() {
            completed.commit
        } else {
            0
        };
        Some(completed)
    }

    /// Acknowledge successful presentation on the UI submission thread. Until
    /// acknowledged, later commits require full replay because their damage
    /// may refer to pixels the user has never seen.
    pub fn acknowledge_presented(&self, key: SceneSurface, commit: u64) -> bool {
        let mut state = self.shared.state.lock().unwrap();
        if state.closed {
            return false;
        }
        let Some(surface) = state.surfaces.get_mut(&key) else {
            return false;
        };
        if surface.removed
            || surface.latest != commit
            || (surface.delivered != commit && surface.presented != commit)
        {
            return false;
        }
        surface.presented = commit;
        true
    }

    /// Check immediately before presentation, on the same UI thread that
    /// submits/removes surfaces. A newer commit invalidates an older output.
    pub fn is_current(&self, key: SceneSurface, commit: u64) -> bool {
        let state = self.shared.state.lock().unwrap();
        !state.closed
            && state
                .surfaces
                .get(&key)
                .is_some_and(|surface| !surface.removed && surface.latest == commit)
    }

    pub fn metrics(&self) -> SceneThreadMetrics {
        let state = self.shared.state.lock().unwrap();
        let mut metrics = state.metrics;
        metrics.surfaces = state.surfaces.len();
        for surface in state.surfaces.values() {
            metrics.pending += usize::from(surface.pending.is_some());
            metrics.completed += usize::from(surface.completed.is_some());
            metrics.in_flight += usize::from(surface.in_flight);
        }
        metrics
    }

    /// Reject future submissions and discard unpublished frames. The active
    /// render finishes on its owning thread before shutdown joins it.
    pub fn close(&self) {
        let mut state = self.shared.state.lock().unwrap();
        state.closed = true;
        state.ready.clear();
        let discarded: Vec<_> = state
            .surfaces
            .values_mut()
            .map(|surface| (surface.pending.take(), surface.completed.take()))
            .collect();
        drop(state);
        drop(discarded);
        self.shared.changed.notify_all();
    }

    pub fn shutdown(mut self) -> thread::Result<()> {
        self.close();
        self.worker
            .take()
            .expect("worker is owned until shutdown")
            .join()
    }
}

impl<B: SceneRenderer> Drop for SceneRenderThread<B> {
    fn drop(&mut self) {
        self.close();
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

fn run_worker<B: SceneRenderer>(
    shared: &Shared<B::Resources, B::Output, B::Error>,
    backend: &mut B,
) {
    loop {
        let (key, frame) = {
            let mut state = shared.state.lock().unwrap();
            while state.ready.is_empty() && !state.closed {
                state = shared.changed.wait(state).unwrap();
            }
            if state.closed {
                return;
            }
            let key = state.ready.pop_front().expect("ready queue is non-empty");
            let surface = state
                .surfaces
                .get_mut(&key)
                .expect("queued surface is retained");
            surface.queued = false;
            if surface.removed {
                drop(state);
                backend.remove_surface(key);
                shared.state.lock().unwrap().surfaces.remove(&key);
                (shared.wake_ui)();
                continue;
            }
            surface.in_flight = true;
            (
                key,
                surface
                    .pending
                    .take()
                    .expect("live queued surface has a frame"),
            )
        };
        let output = backend.render(&frame);
        let publish = {
            let mut state = shared.state.lock().unwrap();
            state.metrics.rendered += 1;
            let closed = state.closed;
            let surface = state
                .surfaces
                .get_mut(&key)
                .expect("in-flight surface is retained");
            surface.in_flight = false;
            if !closed && !surface.removed && surface.latest == frame.commit {
                surface.completed = Some(RenderedScene {
                    surface: key,
                    commit: frame.commit,
                    output,
                });
                true
            } else {
                state.metrics.discarded += 1;
                false
            }
        };
        if publish {
            (shared.wake_ui)();
        }
    }
}

#[cfg(test)]
mod tests;
