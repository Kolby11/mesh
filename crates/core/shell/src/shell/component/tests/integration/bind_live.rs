use super::*;
use crate::shell::ComponentContext;
use crate::shell::component::catalog::FrontendCatalog;
use mesh_core_diagnostics::Diagnostics;
use mesh_core_frontend::CompiledFrontendModule;
use mesh_core_render::PixelBuffer;
use mesh_core_theme::default_theme;

const PARENT_ID: &str = "@test/bind-live";

/// Build a one-surface frontend with a single local `Child` component the parent
/// instantiates with `bind:this`. The catalog points back at the surface itself
/// so the composition resolver can find the local component during render.
fn bind_live_surface(parent_src: &str, child_src: &str) -> FrontendSurfaceComponent {
    let compiled = CompiledFrontendModule {
        manifest: minimal_test_manifest(PARENT_ID),
        source_path: PathBuf::from("src/main.mesh"),
        component: parse_component(parent_src).unwrap(),
        local_components: HashMap::from([("Child".into(), parse_component(child_src).unwrap())]),
        module_component_imports: HashMap::new(),
        watched_paths: Vec::new(),
    };
    let catalog = FrontendCatalog {
        modules: HashMap::from([(
            PARENT_ID.into(),
            FrontendCatalogEntry {
                module_dir: PathBuf::from("."),
                compiled: compiled.clone().into(),
            },
        )]),
        diagnostics: Default::default(),
        extension_point_contributions: HashMap::new(),
        extension_point_entries: HashMap::new(),
        node_slot_placements: Default::default(),
    };
    let mut component = FrontendSurfaceComponent::new(
        compiled,
        PathBuf::from("."),
        catalog,
        InterfaceCatalog::default(),
        test_settings_store(),
    );
    component
        .mount(ComponentContext {
            component_id: PARENT_ID.into(),
            surface_id: PARENT_ID.into(),
            diagnostics: Diagnostics::new(PARENT_ID),
        })
        .unwrap();
    component.visible = true;
    component
}

fn child_runtime_value(
    component: &FrontendSurfaceComponent,
    name: &str,
) -> Option<serde_json::Value> {
    let child_key = format!("{PARENT_ID}/local:Child");
    component
        .runtimes
        .lock()
        .unwrap()
        .get(child_key.as_str())
        .and_then(|runtime| runtime.script_ctx.state().get(name))
}

#[test]
fn bind_this_event_handler_calls_child_live_and_resyncs_it() {
    // A parent event handler calls the child's function through the live
    // `bind:this` reference. Because all components in the surface share one Lua
    // realm, the call runs the child's real function synchronously and returns its
    // real value (no snapshot, no queue); the shell's post-handler re-sync of
    // bound neighbours then surfaces the child's mutation into its own reactive
    // state so the child re-renders.
    let mut component = bind_live_surface(
        r#"
<template>
    <box>
        <Child bind:this={child} />
    </box>
</template>
<script lang="luau">
local Child = require("./components/child.mesh")
observed = -1
function bump()
    observed = child.set_value(99)
end
</script>
"#,
        r#"
<template>
    <box />
</template>
<script lang="luau">
value = 0
function set_value(v)
    value = v
    return value
end
</script>
"#,
    );

    let theme = default_theme();
    let mut buffer = PixelBuffer::new(120, 40);

    // First paint instantiates the child and installs the live binding.
    component
        .paint(&theme, SurfaceExtent::unpadded(120, 40), &mut buffer, 1.0)
        .unwrap();
    assert_eq!(
        runtime_value(&component, "observed"),
        Some(serde_json::json!(-1))
    );
    assert_eq!(
        child_runtime_value(&component, "value"),
        Some(serde_json::json!(0))
    );

    // The parent handler calls the child through the live reference.
    component.call_namespaced_handler("bump", &[]).unwrap();

    // Real synchronous return value (proves the call ran and returned, not queued).
    assert_eq!(
        runtime_value(&component, "observed"),
        Some(serde_json::json!(99))
    );
    // The child's own reactive state reflects the live mutation (proves liveness
    // plus the shell's post-handler re-sync of bound neighbours).
    assert_eq!(
        child_runtime_value(&component, "value"),
        Some(serde_json::json!(99))
    );
}

#[test]
fn bind_this_ordinary_parent_handler_skips_untouched_child_resync() {
    let mut component = bind_live_surface(
        r#"
<template>
    <box>
        <Child bind:this={child} />
    </box>
</template>
<script lang="luau">
local Child = require("./components/child.mesh")
counter = 0
function bump()
    counter = counter + 1
end
</script>
"#,
        r#"
<template>
    <box />
</template>
<script lang="luau">
value = 0
</script>
"#,
    );

    let theme = default_theme();
    let mut buffer = PixelBuffer::new(120, 40);
    component
        .paint(&theme, SurfaceExtent::unpadded(120, 40), &mut buffer, 1.0)
        .unwrap();
    let child_key = format!("{PARENT_ID}/local:Child");

    {
        let mut runtimes = component.runtimes.lock().unwrap();
        let child = runtimes.get_mut(child_key.as_str()).unwrap();
        child.script_ctx.state.set("value", serde_json::json!(123));
        child.script_ctx.state.clear_dirty();
    }

    component.call_namespaced_handler("bump", &[]).unwrap();

    assert_eq!(
        child_runtime_value(&component, "value"),
        Some(serde_json::json!(123)),
        "untouched live-bound children should not be resynced after ordinary parent handlers"
    );
}

// cargo test -p mesh-core-shell --release -- untouched_live_binding_neighbors_skip_resync --ignored --nocapture
#[test]
#[ignore = "release-only live-binding neighbor resync microbenchmark"]
fn untouched_live_binding_neighbors_skip_resync() {
    use std::time::Instant;

    let mut component = bind_live_surface(
        r#"
<template>
    <box>
        <Child bind:this={child} />
    </box>
</template>
<script lang="luau">
local Child = require("./components/child.mesh")
counter = 0
function bump()
    counter = counter + 1
end
</script>
"#,
        r#"
<template>
    <box />
</template>
<script lang="luau">
value = 0
mirror = 0
other = 0
</script>
"#,
    );

    let theme = default_theme();
    let mut buffer = PixelBuffer::new(120, 40);
    component
        .paint(&theme, SurfaceExtent::unpadded(120, 40), &mut buffer, 1.0)
        .unwrap();
    let child_key = format!("{PARENT_ID}/local:Child");
    let iterations = 2_000;

    let old_started = Instant::now();
    for _ in 0..iterations {
        let mut runtimes = component.runtimes.lock().unwrap();
        let child = runtimes.get_mut(child_key.as_str()).unwrap();
        child.script_ctx.resync_state();
        std::hint::black_box(child.script_ctx.state().mutation_generation());
    }
    let old_time = old_started.elapsed();

    let new_started = Instant::now();
    for _ in 0..iterations {
        let mut runtimes = component.runtimes.lock().unwrap();
        let child = runtimes.get_mut(child_key.as_str()).unwrap();
        if child.script_ctx.take_live_binding_external_accessed() {
            child.script_ctx.resync_state();
        }
        std::hint::black_box(child.script_ctx.state().mutation_generation());
    }
    let new_time = new_started.elapsed();

    eprintln!(
        "untouched live-binding neighbor: unconditional resync {old_time:?}; flag skip {new_time:?}; ratio {:.1}x",
        old_time.as_secs_f64() / new_time.as_secs_f64()
    );
    assert!(new_time < old_time);
}

#[test]
fn child_self_event_fire_resyncs_the_parent_that_subscribed() {
    // The reverse direction of `bind_this_event_handler_calls_child_live_and_resyncs_it`:
    // the parent subscribes to the child's `self.<Event>` channel through the
    // live binding, and the child fires it from its own handler. The callback
    // is the parent's closure over the parent's `_ENV`, so the mutation lands
    // in Lua but not in the parent's Rust-side reactive state unless the
    // post-handler neighbour resync knows the parent was touched.
    let mut component = bind_live_surface(
        r#"
<template>
    <box>
        <Child bind:this={child} />
    </box>
</template>
<script lang="luau">
local Child = require("./components/child.mesh")
received = -1
function subscribe()
    child.Changed:on(function(value)
        received = value
    end)
end
</script>
"#,
        r#"
<template>
    <box />
</template>
<script lang="luau">
value = 0
function fire_it()
    value = 7
    self.Changed:fire(7)
end
</script>
"#,
    );

    let theme = default_theme();
    let mut buffer = PixelBuffer::new(120, 40);
    component
        .paint(&theme, SurfaceExtent::unpadded(120, 40), &mut buffer, 1.0)
        .unwrap();

    component.call_namespaced_handler("subscribe", &[]).unwrap();
    assert_eq!(
        runtime_value(&component, "received"),
        Some(serde_json::json!(-1)),
        "subscribing must not itself deliver a value"
    );
    assert!(
        !component
            .runtimes
            .lock()
            .unwrap()
            .get(PARENT_ID)
            .unwrap()
            .script_ctx
            .take_live_binding_external_accessed(),
        "reading and subscribing to the channel must not flag the parent; only a delivered fire does"
    );

    let child_handler = format!("__mesh_embed__::{PARENT_ID}/local:Child::fire_it");
    component
        .call_namespaced_handler(&child_handler, &[])
        .unwrap();

    assert_eq!(
        child_runtime_value(&component, "value"),
        Some(serde_json::json!(7))
    );
    assert_eq!(
        runtime_value(&component, "received"),
        Some(serde_json::json!(7)),
        "the child's fire ran the parent's closure, so the parent's reactive state must resync"
    );
}
