use super::*;
use std::collections::VecDeque;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicUsize, Ordering},
};
use std::time::{Duration, Instant};

struct Item {
    icon: Arc<Mutex<String>>,
    activations: Arc<AtomicUsize>,
}

#[zbus::interface(name = "org.kde.StatusNotifierItem")]
impl Item {
    #[zbus(property)]
    fn id(&self) -> &str {
        "mesh-tray-test"
    }
    #[zbus(property)]
    fn title(&self) -> &str {
        "Test application"
    }
    #[zbus(property)]
    fn status(&self) -> &str {
        "Active"
    }
    #[zbus(property)]
    fn icon_name(&self) -> String {
        self.icon.lock().unwrap().clone()
    }
    #[zbus(property)]
    fn icon_pixmap(&self) -> Vec<(i32, i32, Vec<u8>)> {
        vec![(2, 1, vec![255, 255, 0, 0, 255, 0, 0, 255])]
    }
    fn activate(&self, x: i32, y: i32) {
        assert_eq!((x, y), (12, 34));
        self.activations.fetch_add(1, Ordering::Relaxed);
    }
}

struct Watcher {
    items: Vec<String>,
    hosts: Arc<AtomicUsize>,
}

#[zbus::interface(name = "org.kde.StatusNotifierWatcher")]
impl Watcher {
    #[zbus(property)]
    fn registered_status_notifier_items(&self) -> Vec<String> {
        self.items.clone()
    }
    #[zbus(property)]
    fn is_status_notifier_host_registered(&self) -> bool {
        true
    }
    #[zbus(property)]
    fn protocol_version(&self) -> i32 {
        0
    }
    fn register_status_notifier_host(&self, service: &str) {
        assert_eq!(service, "org.kde.StatusNotifierHost.mesh");
        self.hosts.fetch_add(1, Ordering::Relaxed);
    }
}

struct Harness {
    ctx: BackendScriptContext,
    events: VecDeque<StreamEvent>,
    state: Option<BackendState>,
    commands: Vec<BackendCommandOutcome>,
}

impl Harness {
    fn new() -> Self {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../../modules/backend/status-notifier-tray");
        let manifest: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(root.join("module.json")).unwrap())
                .unwrap();
        let grants = manifest["mesh"]["uses"]["capabilities"]
            .as_array()
            .unwrap()
            .iter()
            .map(|value| value.as_str().unwrap().to_owned())
            .collect::<Vec<_>>();
        let mut ctx =
            BackendScriptContext::new_with_capabilities("@mesh/status-notifier-tray", grants);
        ctx.load_script(&std::fs::read_to_string(root.join("src/main.luau")).unwrap())
            .unwrap();
        let state = ctx.call_init().unwrap();
        Self {
            ctx,
            events: VecDeque::new(),
            state,
            commands: Vec::new(),
        }
    }

    async fn tick(&mut self) {
        self.events.extend(self.ctx.stream_state().drain_events());
        if !self.ctx.stream_in_flight()
            && let Some(event) = self.events.pop_front()
        {
            self.state = self
                .ctx
                .run_stream_event(&event)
                .unwrap()
                .or(self.state.take());
        }
        let run = self.ctx.run_due_tasks(Instant::now());
        assert!(run.errors.is_empty(), "{:?}", run.errors);
        if let Some(start) = run.start {
            start.unwrap();
        }
        for outcome in run.streams {
            outcome.unwrap();
        }
        for outcome in run.commands {
            self.commands.push(outcome.1.unwrap());
        }
        self.state = run.state.or(self.state.take());
        tokio::time::sleep(Duration::from_millis(2)).await;
    }

    async fn until(&mut self, predicate: impl Fn(&Self) -> bool) {
        let deadline = Instant::now() + Duration::from_secs(15);
        while !predicate(self) {
            assert!(
                Instant::now() < deadline,
                "tray did not settle: {:?}",
                self.state
            );
            self.tick().await;
        }
    }

    fn item(&self) -> Option<&serde_json::Value> {
        self.state.as_ref()?.get("items")?.as_array()?.first()
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn tray_provider_registers_updates_activates_and_reuses_an_existing_watcher() {
    if std::env::var_os("MESH_PRIVATE_TRAY_TEST_CHILD").is_none() {
        use std::io::{BufRead, BufReader};
        use std::process::{Command, Stdio};
        let mut daemon = Command::new("dbus-daemon")
            .args(["--session", "--nofork", "--print-address=1"])
            .stdout(Stdio::piped())
            .spawn()
            .expect("dbus-daemon is required");
        let mut address = String::new();
        BufReader::new(daemon.stdout.take().unwrap())
            .read_line(&mut address)
            .unwrap();
        let mut child = Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "backend::tray_tests::tray_provider_registers_updates_activates_and_reuses_an_existing_watcher", "--nocapture"])
            .env("MESH_PRIVATE_TRAY_TEST_CHILD", "1").env("DBUS_SESSION_BUS_ADDRESS", address.trim()).spawn().unwrap();
        let deadline = Instant::now() + Duration::from_secs(45);
        let status = loop {
            if let Some(status) = child.try_wait().unwrap() {
                break Some(status);
            }
            if Instant::now() >= deadline {
                let _ = child.kill();
                let _ = child.wait();
                break None;
            }
            std::thread::sleep(Duration::from_millis(20));
        };
        let _ = daemon.kill();
        let _ = daemon.wait();
        assert!(
            status.is_some_and(|status| status.success()),
            "private-bus tray test failed"
        );
        return;
    }
    let icon = Arc::new(Mutex::new("test-logo".to_string()));
    let activations = Arc::new(AtomicUsize::new(0));
    let peer = zbus::connection::Builder::session()
        .unwrap()
        .serve_at(
            "/StatusNotifierItem",
            Item {
                icon: Arc::clone(&icon),
                activations: Arc::clone(&activations),
            },
        )
        .unwrap()
        .build()
        .await
        .unwrap();
    let mut harness = Harness::new();
    harness
        .until(|h| {
            h.state
                .as_ref()
                .is_some_and(|state| state["available"] == true)
        })
        .await;
    // Host availability precedes the two watcher connections acquiring their names.
    let dbus = zbus::fdo::DBusProxy::new(&peer).await.unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while !dbus
        .name_has_owner("org.kde.StatusNotifierWatcher".try_into().unwrap())
        .await
        .unwrap()
    {
        assert!(Instant::now() < deadline);
        harness.tick().await;
    }
    let registration = tokio::spawn({
        let peer = peer.clone();
        async move {
            peer.call_method(
                Some("org.kde.StatusNotifierWatcher"),
                "/StatusNotifierWatcher",
                Some("org.kde.StatusNotifierWatcher"),
                "RegisterStatusNotifierItem",
                &"/StatusNotifierItem",
            )
            .await
        }
    });
    harness.until(|_| registration.is_finished()).await;
    registration.await.unwrap().unwrap();
    harness.until(|h| h.item().is_some()).await;
    assert_eq!(harness.item().unwrap()["icon_name"], "test-logo");
    let reply = peer
        .call_method(
            Some("org.kde.StatusNotifierWatcher"),
            "/StatusNotifierWatcher",
            Some("org.freedesktop.DBus.Properties"),
            "GetAll",
            &"org.kde.StatusNotifierWatcher",
        )
        .await
        .unwrap();
    let properties = reply
        .body()
        .deserialize::<std::collections::HashMap<String, zbus::zvariant::OwnedValue>>()
        .unwrap();
    assert_eq!(
        bool::try_from(
            properties["IsStatusNotifierHostRegistered"]
                .try_clone()
                .unwrap()
        )
        .unwrap(),
        true
    );
    assert_eq!(
        Vec::<String>::try_from(
            properties["RegisteredStatusNotifierItems"]
                .try_clone()
                .unwrap()
        )
        .unwrap()
        .len(),
        1
    );
    let id = harness.item().unwrap()["id"].as_str().unwrap().to_owned();
    let run = harness
        .ctx
        .run_command_with_result("activate", &serde_json::json!({"id": id, "x": 12, "y": 34}))
        .unwrap();
    if let BackendCommandRun::Finished(outcome) = run {
        harness.commands.push(outcome);
    }
    harness.until(|h| !h.commands.is_empty()).await;
    assert_eq!(
        harness.commands[0].result["ok"], true,
        "{:?}",
        harness.commands[0]
    );
    assert_eq!(activations.load(Ordering::Relaxed), 1);
    *icon.lock().unwrap() = String::new();
    peer.emit_signal(
        None::<&str>,
        "/StatusNotifierItem",
        "org.kde.StatusNotifierItem",
        "NewIcon",
        &(),
    )
    .await
    .unwrap();
    harness
        .until(|h| h.item().is_some_and(|item| item["icon_name"] == ""))
        .await;
    assert_eq!(harness.item().unwrap()["icon_argb"], "ffff0000ff0000ff");
    assert_eq!(harness.item().unwrap()["icon_width"], 2);
    harness.ctx.shutdown_streams().await;
    drop(dbus);

    let hosts = Arc::new(AtomicUsize::new(0));
    let service = format!("{}/StatusNotifierItem", peer.unique_name().unwrap());
    let foreign = zbus::connection::Builder::session()
        .unwrap()
        .name("org.kde.StatusNotifierWatcher")
        .unwrap()
        .serve_at(
            "/StatusNotifierWatcher",
            Watcher {
                items: vec![service],
                hosts: Arc::clone(&hosts),
            },
        )
        .unwrap()
        .build()
        .await
        .unwrap();
    let mut harness = Harness::new();
    harness.until(|h| h.item().is_some()).await;
    harness.until(|_| hosts.load(Ordering::Relaxed) > 0).await;
    let owner = zbus::fdo::DBusProxy::new(&peer)
        .await
        .unwrap()
        .get_name_owner("org.kde.StatusNotifierWatcher".try_into().unwrap())
        .await
        .unwrap();
    assert_eq!(owner.as_str(), foreign.unique_name().unwrap().as_str());
    peer.close().await.unwrap();
    harness.until(|h| h.item().is_none()).await;
    harness.ctx.shutdown_streams().await;
    foreign.close().await.unwrap();
}
