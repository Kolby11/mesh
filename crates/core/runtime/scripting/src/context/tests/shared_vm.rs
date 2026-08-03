use super::super::*;
use super::common::*;
use mesh_core_capability::{Capability, CapabilitySet};
use mesh_core_elements::VariableStore;

#[test]
fn components_sharing_one_vm_keep_isolated_public_members() {
    // Two component instances on a single shared thread VM must keep their
    // public members private to their own _ENV — sharing the VM does not share
    // bare globals (that only happens through an explicit bind:this reference).
    let vm = SurfaceVm::new();

    let mut ctx_a = ScriptContext::new("@mesh/comp-a", CapabilitySet::new()).unwrap();
    ctx_a.attach_shared_vm(&vm);
    ctx_a
        .load_script("secret = \"a-value\"\nfunction init() end")
        .unwrap();
    ctx_a.call_init().unwrap();

    let mut ctx_b = ScriptContext::new("@mesh/comp-b", CapabilitySet::new()).unwrap();
    ctx_b.attach_shared_vm(&vm);
    ctx_b
        .load_script("secret = \"b-value\"\nfunction init() end")
        .unwrap();
    ctx_b.call_init().unwrap();

    assert_eq!(
        ctx_a.state.get("secret"),
        Some(serde_json::json!("a-value"))
    );
    assert_eq!(
        ctx_b.state.get("secret"),
        Some(serde_json::json!("b-value"))
    );
}

#[test]
fn separate_surface_handles_share_thread_vm_without_state_bleed() {
    // SurfaceVm::new is intentionally cheap: both handles refer to the current
    // thread's realm, while each ScriptContext still owns a distinct _ENV.
    let first_surface = SurfaceVm::new();
    let second_surface = SurfaceVm::new();

    let mut first = ScriptContext::new("@mesh/first-surface", CapabilitySet::new()).unwrap();
    first.attach_shared_vm(&first_surface);
    first
        .load_script("surface_value = 'first'\nfunction read() return surface_value end")
        .unwrap();

    let mut second = ScriptContext::new("@mesh/second-surface", CapabilitySet::new()).unwrap();
    second.attach_shared_vm(&second_surface);
    second
        .load_script("surface_value = 'second'\nfunction read() return surface_value end")
        .unwrap();

    assert_eq!(
        first.state.get("surface_value"),
        Some(serde_json::json!("first"))
    );
    assert_eq!(
        second.state.get("surface_value"),
        Some(serde_json::json!("second"))
    );
}

#[test]
fn standalone_contexts_share_thread_vm_without_state_bleed() {
    // Backend/test contexts that do not attach a SurfaceVm use the same
    // thread-owned realm and the same _ENV isolation contract.
    let mut first = ScriptContext::new("@mesh/standalone-a", CapabilitySet::new()).unwrap();
    first.load_script("value = 1").unwrap();
    let mut second = ScriptContext::new("@mesh/standalone-b", CapabilitySet::new()).unwrap();
    second.load_script("value = 2").unwrap();

    assert_eq!(first.state.get("value"), Some(serde_json::json!(1)));
    assert_eq!(second.state.get("value"), Some(serde_json::json!(2)));
}

#[test]
fn thread_vm_reclaims_dropped_context_environments() {
    std::thread::spawn(|| {
        let lua = crate::pool::thread_vm();
        lua.gc_collect().unwrap();
        let baseline = lua.used_memory();

        let contexts: Vec<_> = (0..64)
            .map(|index| {
                let mut context = ScriptContext::new(
                    format!("@mesh/gc-{index}"),
                    CapabilitySet::new(),
                )
                .unwrap();
                context
                    .load_script(&format!(
                        "payload = {{ name = 'context-{index}', values = table.create(128, {index}) }}"
                    ))
                    .unwrap();
                context
            })
            .collect();
        let live = lua.used_memory();
        assert!(live > baseline);

        drop(contexts);
        lua.gc_collect().unwrap();
        lua.gc_collect().unwrap();
        let reclaimed = lua.used_memory();
        assert!(
            reclaimed < live,
            "dropping contexts must make their _ENV graphs collectible"
        );
        let second_wave: Vec<_> = (0..64)
            .map(|index| {
                let mut context =
                    ScriptContext::new(format!("@mesh/gc-{index}"), CapabilitySet::new()).unwrap();
                context
                    .load_script(&format!(
                        "payload = {{ name = 'context-{index}', values = table.create(128, {index}) }}"
                    ))
                    .unwrap();
                context
            })
            .collect();
        drop(second_wave);
        lua.gc_collect().unwrap();
        lua.gc_collect().unwrap();
        let second_reclaimed = lua.used_memory();
        assert!(
            second_reclaimed <= reclaimed.saturating_add(64 * 1024),
            "repeated create/drop waves must not retain another set of _ENV graphs: first={reclaimed}, second={second_reclaimed}"
        );
    })
    .join()
    .unwrap();
}

#[test]
fn interface_event_subscriptions_are_independent_on_shared_vm() {
    // The interface-event channel registry lives on each instance's _ENV, so a
    // subscription on one component must not register on another sharing the VM.
    let vm = SurfaceVm::new();

    let mut subscriber_caps = CapabilitySet::new();
    subscriber_caps.grant(Capability::new("service.audio.read"));
    let mut subscriber = ScriptContext::new("@mesh/subscriber", subscriber_caps).unwrap();
    subscriber.attach_shared_vm(&vm);
    subscriber.set_interface_catalog(audio_catalog());
    subscriber
        .load_script(
            r#"
function init()
    local audio = require("mesh.audio@>=1.0")
    audio.events.VolumeChanged:subscribe(function(_event) end)
end
"#,
        )
        .unwrap();
    subscriber.call_init().unwrap();

    let mut idle_caps = CapabilitySet::new();
    idle_caps.grant(Capability::new("service.audio.read"));
    let mut idle = ScriptContext::new("@mesh/idle", idle_caps).unwrap();
    idle.attach_shared_vm(&vm);
    idle.set_interface_catalog(audio_catalog());
    idle.load_script("function init() end").unwrap();
    idle.call_init().unwrap();

    assert!(subscriber.is_subscribed_to_interface_event("audio", "VolumeChanged"));
    assert!(!idle.has_interface_event_subscription_for_service("audio"));
}

#[test]
fn same_component_on_shared_vm_has_independent_self_channels() {
    // Two instances of the SAME component (same module_id) on one VM must not
    // share `self.Changed` — the regression that motivated moving the self-event
    // registry from globals to the per-instance _ENV.
    let vm = SurfaceVm::new();

    fn instance(vm: &SurfaceVm) -> ScriptContext {
        let mut ctx = ScriptContext::new("@mesh/item-row", CapabilitySet::new()).unwrap();
        ctx.attach_shared_vm(vm);
        ctx.load_script(
            r#"
hits = 0
function init(self)
    self.Changed:on(function(_event) hits = hits + 1 end)
    self.Changed:fire({})
end
"#,
        )
        .unwrap();
        ctx.call_init().unwrap();
        ctx
    }

    let first = instance(&vm);
    let second = instance(&vm);

    // Each instance's own fire incremented only its own counter. If the channels
    // collided, the second instance's fire would also run the first's handler.
    assert_eq!(first.state.get("hits"), Some(serde_json::json!(1)));
    assert_eq!(second.state.get("hits"), Some(serde_json::json!(1)));
}

#[test]
fn live_binding_reads_and_calls_child_in_same_tick() {
    // A live `bind:this` proxy forwards straight to the child's `_ENV` in the
    // shared VM: the parent reads the child's current value and calls its real
    // function synchronously, with the real return value — no snapshot, no queue.
    let vm = SurfaceVm::new();

    let mut child = ScriptContext::new("@mesh/slider", CapabilitySet::new()).unwrap();
    child.attach_shared_vm(&vm);
    child
        .load_script(
            r#"
percent = 10
function set_volume(value)
    percent = value
    return percent
end
function init() end
"#,
        )
        .unwrap();
    child.call_init().unwrap();

    let mut parent = ScriptContext::new("@mesh/host", CapabilitySet::new()).unwrap();
    parent.attach_shared_vm(&vm);
    parent
        .load_script(
            r#"
returned = 0
observed = 0
function bump()
    returned = slider.set_volume(77)
    observed = slider.percent
end
function init() end
"#,
        )
        .unwrap();
    parent.call_init().unwrap();

    parent.install_live_binding("slider", &child).unwrap();
    parent.call_handler("bump", &[]).unwrap();

    // The bound call ran the child's real function and returned its real value.
    assert_eq!(parent.state.get("returned"), Some(serde_json::json!(77)));
    // The parent read the value the child mutated within the same tick (liveness).
    assert_eq!(parent.state.get("observed"), Some(serde_json::json!(77)));

    // Re-syncing the child surfaces the live `_ENV` mutation into its own state.
    child.resync_state();
    assert_eq!(child.state.get("percent"), Some(serde_json::json!(77)));
}

#[test]
fn live_binding_does_not_expose_host_internals() {
    // The live proxy is curated: host internals (`self`, `module`, `mesh`,
    // `require`, `__mesh_*`) and lifecycle hooks must not cross the boundary,
    // only the child's public members do.
    let vm = SurfaceVm::new();

    let mut child = ScriptContext::new("@mesh/child", CapabilitySet::new()).unwrap();
    child.attach_shared_vm(&vm);
    child
        .load_script("public_value = 5\nfunction init() end")
        .unwrap();
    child.call_init().unwrap();

    let mut parent = ScriptContext::new("@mesh/parent", CapabilitySet::new()).unwrap();
    parent.attach_shared_vm(&vm);
    parent
        .load_script(
            r#"
has_public = false
has_self = true
has_require = true
has_mesh = true
function probe()
    has_public = child.public_value == 5
    has_self = child.self ~= nil
    has_require = child.require ~= nil
    has_mesh = child.mesh ~= nil
end
function init() end
"#,
        )
        .unwrap();
    parent.call_init().unwrap();

    parent.install_live_binding("child", &child).unwrap();
    parent.call_handler("probe", &[]).unwrap();

    assert_eq!(
        parent.state.get("has_public"),
        Some(serde_json::json!(true))
    );
    assert_eq!(parent.state.get("has_self"), Some(serde_json::json!(false)));
    assert_eq!(
        parent.state.get("has_require"),
        Some(serde_json::json!(false))
    );
    assert_eq!(parent.state.get("has_mesh"), Some(serde_json::json!(false)));
}

#[test]
fn live_binding_routes_child_self_event_to_parent_in_same_tick() {
    // Child→parent events: the live proxy exposes the child's `self.<Event>`
    // channel, so a parent subscribes with `child.Event:on(fn)` and the child's
    // `self.Event:fire(...)` runs the parent's closure synchronously — same
    // channel table in the shared VM, no marshalling.
    let vm = SurfaceVm::new();

    let mut child = ScriptContext::new("@mesh/emitter", CapabilitySet::new()).unwrap();
    child.attach_shared_vm(&vm);
    child
        .load_script(
            r#"
local changed
function init(self)
    changed = self.Changed
end
function announce()
    changed:fire({ value = 42 })
end
"#,
        )
        .unwrap();
    child.call_init().unwrap();

    let mut parent = ScriptContext::new("@mesh/listener", CapabilitySet::new()).unwrap();
    parent.attach_shared_vm(&vm);
    parent
        .load_script(
            r#"
received = 0
function listen()
    emitter.Changed:on(function(event) received = event.value end)
end
function init() end
"#,
        )
        .unwrap();
    parent.call_init().unwrap();

    parent.install_live_binding("emitter", &child).unwrap();

    // Parent registers a real Lua closure on the child's live self-event channel.
    parent.call_handler("listen", &[]).unwrap();
    assert_eq!(parent.state.get("received"), Some(serde_json::json!(0)));

    // The child fires; the parent's closure runs synchronously in the same tick.
    child.call_handler("announce", &[]).unwrap();

    // The parent's `_ENV` was mutated by the fired callback; re-syncing surfaces
    // it into the parent's reactive state (what the shell does after the handler).
    parent.resync_state();
    assert_eq!(parent.state.get("received"), Some(serde_json::json!(42)));
}
