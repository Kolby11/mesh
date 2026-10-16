use super::*;
use std::rc::Rc;
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::Duration;

struct Resources(u64);
impl SceneResources for Resources {
    fn revision(&self) -> u64 {
        self.0
    }
}

fn surface(id: u64) -> SceneSurface {
    SceneSurface { id, incarnation: 1 }
}
fn snapshot(id: u64, resources: Arc<Resources>) -> SceneSnapshot<Resources> {
    let mut plan = FramePaintPlan::empty();
    plan.inputs.surface_size = Some((32, 32));
    plan.inputs.resource_revision = resources.revision();
    SceneSnapshot::new(
        surface(id),
        Arc::new(plan),
        resources,
        FractionalScale::new(1.25),
    )
    .unwrap()
}

struct Renderer {
    _thread_affinity: Rc<()>,
    first: bool,
    started: Sender<()>,
    release: Receiver<()>,
    removed: Sender<SceneSurface>,
    dropped: Sender<thread::ThreadId>,
}
impl SceneRenderer for Renderer {
    type Resources = Resources;
    type Output = (u64, bool);
    type Error = &'static str;
    fn render(&mut self, frame: &SceneFrame<Resources>) -> Result<Self::Output, Self::Error> {
        if self.first {
            self.first = false;
            self.started.send(()).unwrap();
            self.release
                .recv_timeout(Duration::from_secs(2))
                .map_err(|_| "test barrier timed out")?;
        }
        Ok((frame.commit(), frame.requires_full_replay()))
    }
    fn remove_surface(&mut self, key: SceneSurface) {
        self.removed.send(key).unwrap();
    }
}
impl Drop for Renderer {
    fn drop(&mut self) {
        let _ = self.dropped.send(thread::current().id());
    }
}

type Worker = SceneRenderThread<Renderer>;
struct Harness {
    worker: Worker,
    started: Receiver<()>,
    release: Sender<()>,
    completed: Receiver<()>,
    removed: Receiver<SceneSurface>,
    dropped: Receiver<thread::ThreadId>,
}
fn harness(capacity: usize) -> Harness {
    let (started, did_start) = mpsc::channel();
    let (release, released) = mpsc::channel();
    let (completed, did_complete) = mpsc::channel();
    let (removed, did_remove) = mpsc::channel();
    let (dropped, did_drop) = mpsc::channel();
    let worker = Worker::spawn(
        capacity,
        move || {
            Ok(Renderer {
                _thread_affinity: Rc::new(()),
                first: true,
                started,
                release: released,
                removed,
                dropped,
            })
        },
        move || {
            let _ = completed.send(());
        },
    )
    .unwrap();
    Harness {
        worker,
        started: did_start,
        release,
        completed: did_complete,
        removed: did_remove,
        dropped: did_drop,
    }
}
fn receive<T>(channel: &Receiver<T>) -> T {
    channel
        .recv_timeout(Duration::from_secs(3))
        .expect("worker progress")
}

#[test]
fn coalescing_keeps_latest_scene_and_repairs_dropped_frame_damage() {
    let harness = harness(2);
    let resources = Arc::new(Resources(7));
    let weak = Arc::downgrade(&resources);
    let first = harness.worker.submit(snapshot(1, resources)).unwrap();
    receive(&harness.started);
    assert!(weak.upgrade().is_some(), "active frame pins its resources");
    let second = harness
        .worker
        .submit(snapshot(1, Arc::new(Resources(7))))
        .unwrap();
    let third = harness
        .worker
        .submit(snapshot(1, Arc::new(Resources(7))))
        .unwrap();
    assert!(third > second && second > first);
    assert!(!harness.worker.is_current(surface(1), first));
    assert!(!harness.worker.acknowledge_presented(surface(1), third));
    let metrics = harness.worker.metrics();
    assert_eq!(metrics.pending, 1);
    assert_eq!(metrics.in_flight, 1);
    assert_eq!(metrics.coalesced, 1);
    harness.release.send(()).unwrap();
    receive(&harness.completed);
    let completed = harness.worker.take_completed(surface(1)).unwrap();
    assert_eq!(completed.commit, third);
    assert_eq!(completed.output.unwrap(), (third, true));
    assert!(
        weak.upgrade().is_none(),
        "superseded active frame released its lease"
    );
    assert!(harness.worker.acknowledge_presented(surface(1), third));
    let fourth = harness
        .worker
        .submit(snapshot(1, Arc::new(Resources(7))))
        .unwrap();
    receive(&harness.completed);
    assert_eq!(
        harness
            .worker
            .take_completed(surface(1))
            .unwrap()
            .output
            .unwrap(),
        (fourth, false)
    );
    let fifth = harness
        .worker
        .submit(snapshot(1, Arc::new(Resources(7))))
        .unwrap();
    receive(&harness.completed);
    assert_eq!(
        harness
            .worker
            .take_completed(surface(1))
            .unwrap()
            .output
            .unwrap(),
        (fifth, true)
    );
    assert!(!harness.worker.acknowledge_presented(surface(1), fourth));
    let metrics = harness.worker.metrics();
    assert_eq!(metrics.rendered, 4);
    assert_eq!(metrics.discarded, 1);
    harness.worker.shutdown().unwrap();
    assert_ne!(receive(&harness.dropped), thread::current().id());
}

#[test]
fn removed_surfaces_never_publish_and_hold_capacity_until_retired() {
    let harness = harness(1);
    harness
        .worker
        .submit(snapshot(1, Arc::new(Resources(0))))
        .unwrap();
    receive(&harness.started);
    assert!(harness.worker.remove_surface(surface(1)));
    assert!(!harness.worker.remove_surface(surface(1)));
    assert_eq!(
        harness.worker.submit(snapshot(2, Arc::new(Resources(0)))),
        Err(SceneSubmitError::SurfaceCapacity)
    );
    assert_eq!(
        harness.worker.submit(snapshot(1, Arc::new(Resources(0)))),
        Err(SceneSubmitError::SurfaceRemoved)
    );
    harness.release.send(()).unwrap();
    assert_eq!(receive(&harness.removed), surface(1));
    receive(&harness.completed);
    assert_eq!(harness.worker.metrics().surfaces, 0);
    assert!(harness.worker.take_completed(surface(1)).is_none());
    harness
        .worker
        .submit(snapshot(2, Arc::new(Resources(0))))
        .unwrap();
    receive(&harness.completed);
    assert!(harness.worker.take_completed(surface(2)).is_some());
    harness.worker.shutdown().unwrap();
}

#[test]
fn close_discards_active_results_and_drops_backend_on_its_owner() {
    let harness = harness(1);
    let commit = harness
        .worker
        .submit(snapshot(1, Arc::new(Resources(0))))
        .unwrap();
    receive(&harness.started);
    harness.worker.close();
    assert_eq!(
        harness.worker.submit(snapshot(1, Arc::new(Resources(0)))),
        Err(SceneSubmitError::Closed)
    );
    assert!(!harness.worker.is_current(surface(1), commit));
    harness.release.send(()).unwrap();
    harness.worker.shutdown().unwrap();
    assert_ne!(receive(&harness.dropped), thread::current().id());
    assert!(harness.completed.try_recv().is_err());
}

#[test]
fn scene_snapshot_pins_a_plan_and_rejects_mismatched_resources() {
    let mut plan = FramePaintPlan::empty();
    plan.inputs.surface_size = Some((32, 32));
    let mut plan = Arc::new(plan);
    let snapshot = SceneSnapshot::new(
        surface(1),
        Arc::clone(&plan),
        Arc::new(Resources(0)),
        FractionalScale::new(1.0),
    )
    .unwrap();
    Arc::make_mut(&mut plan).inputs.surface_size = Some((64, 64));
    assert_eq!(snapshot.plan().inputs.surface_size, Some((32, 32)));
    assert!(matches!(
        SceneSnapshot::new(
            surface(1),
            plan,
            Arc::new(Resources(1)),
            FractionalScale::new(1.0)
        ),
        Err(SceneSubmitError::ResourceRevisionMismatch)
    ));
    assert!(matches!(
        SceneSnapshot::new(
            surface(1),
            Arc::new(FramePaintPlan::empty()),
            Arc::new(Resources(0)),
            FractionalScale::new(1.0)
        ),
        Err(SceneSubmitError::MissingSurfaceExtent)
    ));
}

#[test]
fn scene_commit_burst_preserves_the_queue_budget() {
    let harness = harness(1);
    harness
        .worker
        .submit(snapshot(1, Arc::new(Resources(0))))
        .unwrap();
    receive(&harness.started);
    let mut latest = 0;
    for _ in 0..1_000 {
        latest = harness
            .worker
            .submit(snapshot(1, Arc::new(Resources(0))))
            .unwrap();
    }
    let metrics = harness.worker.metrics();
    assert_eq!(metrics.surfaces, 1);
    assert_eq!(metrics.pending, 1);
    assert_eq!(metrics.in_flight, 1);
    assert_eq!(metrics.completed, 0);
    assert_eq!(metrics.coalesced, 999);
    harness.release.send(()).unwrap();
    receive(&harness.completed);
    assert_eq!(
        harness.worker.take_completed(surface(1)).unwrap().commit,
        latest
    );
    assert_eq!(harness.worker.metrics().rendered, 2);
    harness.worker.shutdown().unwrap();
}

#[test]
fn scale_resource_and_extent_changes_require_full_replay() {
    let harness = harness(1);
    let commit = harness
        .worker
        .submit(snapshot(1, Arc::new(Resources(0))))
        .unwrap();
    receive(&harness.started);
    harness.release.send(()).unwrap();
    receive(&harness.completed);
    harness.worker.take_completed(surface(1)).unwrap();
    assert!(harness.worker.acknowledge_presented(surface(1), commit));
    for (revision, scale, extent) in [(1, 1.25, (32, 32)), (1, 2.0, (32, 32)), (1, 2.0, (64, 64))] {
        let mut scene = snapshot(1, Arc::new(Resources(revision)));
        scene.scale = FractionalScale::new(scale);
        Arc::make_mut(&mut scene.plan).inputs.surface_size = Some(extent);
        let commit = harness.worker.submit(scene).unwrap();
        receive(&harness.completed);
        assert_eq!(
            harness
                .worker
                .take_completed(surface(1))
                .unwrap()
                .output
                .unwrap(),
            (commit, true)
        );
        assert!(harness.worker.acknowledge_presented(surface(1), commit));
    }
    harness.worker.shutdown().unwrap();
}

struct FailingRenderer(bool);
impl SceneRenderer for FailingRenderer {
    type Resources = Resources;
    type Output = bool;
    type Error = &'static str;
    fn render(&mut self, frame: &SceneFrame<Resources>) -> Result<bool, &'static str> {
        if std::mem::replace(&mut self.0, false) {
            Err("render failed")
        } else {
            Ok(frame.requires_full_replay())
        }
    }
    fn remove_surface(&mut self, _: SceneSurface) {}
}

#[test]
fn render_failure_cannot_acknowledge_pixels_and_the_next_frame_recovers() {
    let (completed, ready) = mpsc::channel();
    let worker = SceneRenderThread::spawn(
        1,
        || Ok(FailingRenderer(true)),
        move || {
            let _ = completed.send(());
        },
    )
    .unwrap();
    let failed = worker.submit(snapshot(1, Arc::new(Resources(0)))).unwrap();
    receive(&ready);
    assert_eq!(
        worker.take_completed(surface(1)).unwrap().output,
        Err("render failed")
    );
    assert!(!worker.acknowledge_presented(surface(1), failed));
    let recovered = worker.submit(snapshot(1, Arc::new(Resources(0)))).unwrap();
    receive(&ready);
    assert_eq!(worker.take_completed(surface(1)).unwrap().output, Ok(true));
    assert!(worker.acknowledge_presented(surface(1), recovered));
    worker.shutdown().unwrap();
}

#[test]
fn initialization_errors_reach_the_caller() {
    let result = SceneRenderThread::<FailingRenderer>::spawn(1, || Err("GPU unavailable"), || {});
    assert!(matches!(
        result,
        Err(SceneThreadStartError::Initialize("GPU unavailable"))
    ));
    let result =
        SceneRenderThread::<FailingRenderer>::spawn(0, || Ok(FailingRenderer(false)), || {});
    assert!(matches!(
        result,
        Err(SceneThreadStartError::InvalidCapacity)
    ));
}

struct LeasedOutput(Arc<dyn Fn() + Send + Sync>);
impl Drop for LeasedOutput {
    fn drop(&mut self) {
        (self.0)();
    }
}
struct LeaseRenderer(Arc<dyn Fn() + Send + Sync>);
impl SceneRenderer for LeaseRenderer {
    type Resources = Resources;
    type Output = LeasedOutput;
    type Error = &'static str;
    fn render(&mut self, _: &SceneFrame<Resources>) -> Result<LeasedOutput, &'static str> {
        Ok(LeasedOutput(Arc::clone(&self.0)))
    }
    fn remove_surface(&mut self, _: SceneSurface) {}
}

#[test]
fn discarded_output_leases_retire_outside_the_queue_lock() {
    let slot = Arc::new(Mutex::new(
        None::<std::sync::Weak<Shared<Resources, LeasedOutput, &'static str>>>,
    ));
    let weak_slot = Arc::clone(&slot);
    let (retired, did_retire) = mpsc::channel();
    let retirement = Arc::new(move || {
        let shared = weak_slot
            .lock()
            .unwrap()
            .as_ref()
            .unwrap()
            .upgrade()
            .unwrap();
        let unlocked = shared.state.try_lock().is_ok();
        retired.send(unlocked).unwrap();
    });
    let (completed, ready) = mpsc::channel();
    let worker = SceneRenderThread::spawn(
        1,
        move || Ok(LeaseRenderer(retirement)),
        move || {
            let _ = completed.send(());
        },
    )
    .unwrap();
    *slot.lock().unwrap() = Some(Arc::downgrade(&worker.shared));
    worker.submit(snapshot(1, Arc::new(Resources(0)))).unwrap();
    receive(&ready);
    worker.submit(snapshot(1, Arc::new(Resources(0)))).unwrap();
    assert!(
        receive(&did_retire),
        "superseded output may re-enter its resource owner"
    );
    receive(&ready);
    worker.close();
    assert!(
        receive(&did_retire),
        "shutdown retirement may re-enter its resource owner"
    );
    worker.shutdown().unwrap();
}
