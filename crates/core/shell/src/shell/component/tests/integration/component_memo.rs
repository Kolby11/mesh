use super::*;
use crate::shell::ComponentContext;
use crate::shell::component::catalog::{FrontendCatalog, ResolvedSlotContribution};
use mesh_core_diagnostics::Diagnostics;
use mesh_core_frontend::CompiledFrontendModule;
use mesh_core_render::PixelBuffer;
use mesh_core_theme::default_theme;

const PARENT_ID: &str = "@test/memo-surface";

/// Build a one-surface frontend whose catalog points back at the surface
/// itself so local PascalCase components resolve during render.
fn memo_surface(parent_src: &str, locals: &[(&str, &str)]) -> FrontendSurfaceComponent {
    let compiled = CompiledFrontendModule {
        manifest: minimal_test_manifest(PARENT_ID),
        source_path: PathBuf::from("src/main.mesh"),
        component: parse_component(parent_src).unwrap(),
        local_components: locals
            .iter()
            .map(|(alias, src)| ((*alias).to_string(), parse_component(src).unwrap()))
            .collect(),
        module_component_imports: HashMap::new(),
        watched_paths: Vec::new(),
    };
    let catalog = FrontendCatalog {
        modules: HashMap::from([(
            PARENT_ID.into(),
            FrontendCatalogEntry {
                module_dir: PathBuf::from("."),
                compiled: compiled.clone(),
            },
        )]),
        slot_contributions: HashMap::new(),
    };
    let mut component = FrontendSurfaceComponent::new(
        compiled,
        PathBuf::from("."),
        catalog,
        InterfaceCatalog::default(),
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

fn node_with_content<'a>(node: &'a WidgetNode, content: &str) -> Option<&'a WidgetNode> {
    if node.attributes.get("content").is_some_and(|c| c == content) {
        return Some(node);
    }
    node.children
        .iter()
        .find_map(|child| node_with_content(child, content))
}

/// Force a full script-path rebuild and paint. The first build runs before
/// layout exists, so embedded container sizes settle only from the second
/// build onward — tests warm the memo with one extra rebuild before measuring.
fn rebuild(component: &mut FrontendSurfaceComponent, width: u32, height: u32) -> PixelBuffer {
    let theme = default_theme();
    let mut buffer = PixelBuffer::new(width, height);
    component.invalidate_script_state();
    component
        .paint(&theme, width, height, &mut buffer, 1.0)
        .unwrap();
    buffer
}

fn runtime_value(
    component: &FrontendSurfaceComponent,
    instance_key: &str,
    name: &str,
) -> Option<serde_json::Value> {
    component
        .runtimes
        .lock()
        .unwrap()
        .get(instance_key)
        .and_then(|runtime| runtime.script_ctx.state().get(name))
}

#[test]
fn unchanged_local_component_rebuild_reuses_memoized_subtree() {
    let mut component = memo_surface(
        r#"
<template>
    <box>
        <Child label="static" />
    </box>
</template>
<script lang="luau">
local Child = require("./components/child.mesh")
</script>
"#,
        &[(
            "Child",
            r#"
<template>
    <text content="{label}" />
</template>
<script lang="luau">
label = ""
</script>
"#,
        )],
    );

    let theme = default_theme();
    let mut initial = PixelBuffer::new(160, 60);
    component.paint(&theme, 160, 60, &mut initial, 1.0).unwrap();
    // Warm-up: the second build settles embedded container sizes post-layout.
    let first = rebuild(&mut component, 160, 60);
    let hits_before = component.component_memo_hit_count();

    // A script-state invalidation with no actual changes rebuilds the surface
    // tree; the untouched child must come from the memo, producing identical
    // pixels.
    let second = rebuild(&mut component, 160, 60);
    assert_eq!(component.component_memo_hit_count(), hits_before + 1);
    assert_eq!(first.data, second.data);
    assert!(
        node_with_content(component.last_tree.as_ref().unwrap(), "static").is_some(),
        "memoized child subtree still renders its content"
    );
}

#[test]
fn repeated_local_aliases_have_independent_runtimes_and_memo_entries() {
    let mut component = memo_surface(
        r#"
<template>
    <row>
        <Child label="first" />
        <Child label="second" />
    </row>
</template>
<script lang="luau">
local Child = require("./components/child.mesh")
</script>
"#,
        &[((
            "Child",
            r#"
<template><text content="{label}" /></template>
<script lang="luau">label = ""</script>
"#,
        ))],
    );

    let theme = default_theme();
    let mut initial = PixelBuffer::new(240, 60);
    component.paint(&theme, 240, 60, &mut initial, 1.0).unwrap();
    rebuild(&mut component, 240, 60);

    assert_eq!(
        runtime_value(&component, "@test/memo-surface/local:Child#0", "label"),
        Some(serde_json::json!("first"))
    );
    assert_eq!(
        runtime_value(&component, "@test/memo-surface/local:Child#1", "label"),
        Some(serde_json::json!("second"))
    );

    let hits_before = component.component_memo_hit_count();
    rebuild(&mut component, 240, 60);
    assert_eq!(component.component_memo_hit_count(), hits_before + 2);
    let tree = component.last_tree.as_ref().unwrap();
    assert!(node_with_content(tree, "first").is_some());
    assert!(node_with_content(tree, "second").is_some());
}

#[test]
fn loop_rendered_local_aliases_have_independent_positional_instances() {
    let mut component = memo_surface(
        r#"
<template>
    {#for item in items}
        <Child label={item.label} />
    {/for}
</template>
<script lang="luau">
local Child = require("./components/child.mesh")
items = { { label = "first" }, { label = "second" } }
</script>
"#,
        &[((
            "Child",
            r#"
<template><text content="{label}" /></template>
<script lang="luau">label = ""</script>
"#,
        ))],
    );

    let theme = default_theme();
    let mut initial = PixelBuffer::new(240, 60);
    component.paint(&theme, 240, 60, &mut initial, 1.0).unwrap();
    rebuild(&mut component, 240, 60);

    assert_eq!(
        runtime_value(&component, "@test/memo-surface/local:Child@0", "label"),
        Some(serde_json::json!("first"))
    );
    assert_eq!(
        runtime_value(&component, "@test/memo-surface/local:Child@1", "label"),
        Some(serde_json::json!("second"))
    );

    let hits_before = component.component_memo_hit_count();
    rebuild(&mut component, 240, 60);
    assert_eq!(component.component_memo_hit_count(), hits_before + 2);
}

// cargo test -p mesh-core-shell --release -- repeated_alias_memoization_beats_forced_misses --ignored --nocapture
#[test]
#[ignore = "release-only repeated-alias component memoization benchmark"]
fn repeated_alias_memoization_beats_forced_misses() {
    let children = (0..12)
        .map(|index| format!("        <Child label=\"item {index}\" />\n"))
        .collect::<String>();
    let parent_src = format!(
        r#"
<template>
    <column>
{children}    </column>
</template>
<script lang="luau">
local Child = require("./components/child.mesh")
</script>
"#
    );
    let mut component = memo_surface(
        &parent_src,
        &[((
            "Child",
            r#"
<template><text content="{label}" /></template>
<script lang="luau">label = ""</script>
"#,
        ))],
    );
    let theme = default_theme();
    let mut buffer = PixelBuffer::new(400, 300);
    component.paint(&theme, 400, 300, &mut buffer, 1.0).unwrap();
    rebuild(&mut component, 400, 300);

    let iterations = 200;
    let miss_started = std::time::Instant::now();
    for _ in 0..iterations {
        component.clear_component_memo();
        rebuild(&mut component, 400, 300);
    }
    let miss_time = miss_started.elapsed();

    rebuild(&mut component, 400, 300);
    let hits_before = component.component_memo_hit_count();
    let hit_started = std::time::Instant::now();
    for _ in 0..iterations {
        rebuild(&mut component, 400, 300);
    }
    let hit_time = hit_started.elapsed();
    let hits = component.component_memo_hit_count() - hits_before;

    eprintln!(
        "repeated-alias memoization: forced misses {miss_time:?}; memoized {hit_time:?}; ratio {:.2}x; hits={hits}",
        miss_time.as_secs_f64() / hit_time.as_secs_f64()
    );
    assert_eq!(hits, iterations * 12);
    assert!(hit_time < miss_time);
}

fn memo_slot_surface(contribution_count: usize) -> FrontendSurfaceComponent {
    let mut parent_manifest = minimal_test_manifest(PARENT_ID);
    parent_manifest
        .provides_slots
        .insert("main".into(), mesh_core_module::SlotDefinition::default());
    let parent_compiled = CompiledFrontendModule {
        manifest: parent_manifest,
        source_path: PathBuf::from("src/main.mesh"),
        component: parse_component("<template><slot name=\"main\"/></template>").unwrap(),
        local_components: HashMap::new(),
        module_component_imports: HashMap::new(),
        watched_paths: Vec::new(),
    };
    let widget_id = "@test/memo-slot-widget";
    let mut widget_manifest = minimal_test_manifest(widget_id);
    widget_manifest.package.module_type = mesh_core_module::ModuleType::Widget;
    let widget_compiled = CompiledFrontendModule {
        manifest: widget_manifest,
        source_path: PathBuf::from("src/main.mesh"),
        component: parse_component("<template><text content=\"{label}\"/></template>").unwrap(),
        local_components: HashMap::new(),
        module_component_imports: HashMap::new(),
        watched_paths: Vec::new(),
    };
    let catalog = FrontendCatalog {
        modules: HashMap::from([
            (
                PARENT_ID.into(),
                FrontendCatalogEntry {
                    module_dir: PathBuf::from("."),
                    compiled: parent_compiled.clone(),
                },
            ),
            (
                widget_id.into(),
                FrontendCatalogEntry {
                    module_dir: PathBuf::from("."),
                    compiled: widget_compiled,
                },
            ),
        ]),
        slot_contributions: HashMap::from([(
            format!("{PARENT_ID}:main"),
            (0..contribution_count)
                .map(|index| {
                    let props = serde_json::Map::from_iter([(
                        "label".into(),
                        serde_json::Value::String(format!("static slot {index}")),
                    )]);
                    ResolvedSlotContribution {
                        source_module_id: "@test/contributor".into(),
                        widget_id: widget_id.into(),
                        contribution_id: format!("status-{index}"),
                        order: index as i64,
                        props_fingerprint: crate::shell::component::memo::slot_props_fingerprint(
                            &props,
                        ),
                        props,
                    }
                })
                .collect(),
        )]),
    };
    let mut component = FrontendSurfaceComponent::new(
        parent_compiled,
        PathBuf::from("."),
        catalog,
        InterfaceCatalog::default(),
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

#[test]
fn unchanged_slot_contribution_reuses_memoized_subtree() {
    let mut component = memo_slot_surface(1);

    let theme = default_theme();
    let mut initial = PixelBuffer::new(160, 60);
    component.paint(&theme, 160, 60, &mut initial, 1.0).unwrap();
    rebuild(&mut component, 160, 60);
    let hits_before = component.component_memo_hit_count();
    let second = rebuild(&mut component, 160, 60);

    assert_eq!(component.component_memo_hit_count(), hits_before + 1);
    assert_eq!(initial.data, second.data);
    assert!(node_with_content(component.last_tree.as_ref().unwrap(), "static slot 0").is_some());
}

// cargo test -p mesh-core-shell --release -- slot_memoized_rebuild_beats_full_reeval --ignored --nocapture
#[test]
#[ignore = "release-only slot contribution render memoization benchmark"]
fn slot_memoized_rebuild_beats_full_reeval() {
    let mut component = memo_slot_surface(12);
    let theme = default_theme();
    let mut buffer = PixelBuffer::new(400, 300);
    component.paint(&theme, 400, 300, &mut buffer, 1.0).unwrap();
    rebuild(&mut component, 400, 300);
    let iterations = 200;

    let miss_started = std::time::Instant::now();
    for _ in 0..iterations {
        component.clear_component_memo();
        rebuild(&mut component, 400, 300);
    }
    let miss_time = miss_started.elapsed();

    rebuild(&mut component, 400, 300);
    let hits_before = component.component_memo_hit_count();
    let hit_started = std::time::Instant::now();
    for _ in 0..iterations {
        rebuild(&mut component, 400, 300);
    }
    let hit_time = hit_started.elapsed();
    let hits = component.component_memo_hit_count() - hits_before;

    eprintln!(
        "slot memoization: forced-miss rebuilds {miss_time:?}; memoized rebuilds {hit_time:?}; ratio {:.1}x; hits={hits}",
        miss_time.as_secs_f64() / hit_time.as_secs_f64()
    );
    assert_eq!(hits, iterations * 12);
    assert!(hit_time < miss_time);
}

#[test]
fn changed_prop_rebuilds_child_but_reuses_unchanged_sibling() {
    let mut component = memo_surface(
        r#"
<template>
    <box>
        <Child label="{parent_label}" />
        <Other />
    </box>
</template>
<script lang="luau">
local Child = require("./components/child.mesh")
local Other = require("./components/other.mesh")
parent_label = "one"
function setLabel()
    parent_label = "two"
end
</script>
"#,
        &[
            (
                "Child",
                r#"
<template>
    <text content="{label}" />
</template>
<script lang="luau">
label = ""
</script>
"#,
            ),
            (
                "Other",
                r#"
<template>
    <text content="sibling" />
</template>
"#,
            ),
        ],
    );

    let theme = default_theme();
    let mut buffer = PixelBuffer::new(200, 60);
    component.paint(&theme, 200, 60, &mut buffer, 1.0).unwrap();
    // Warm-up: settle container sizes and populate the memo.
    rebuild(&mut component, 200, 60);
    let hits_before = component.component_memo_hit_count();

    component.call_namespaced_handler("setLabel", &[]).unwrap();
    component.paint(&theme, 200, 60, &mut buffer, 1.0).unwrap();

    // The prop-changed child missed the memo and rebuilt with the new label;
    // the untouched sibling was served from the memo.
    assert_eq!(component.component_memo_hit_count(), hits_before + 1);
    assert_eq!(
        runtime_value(&component, &format!("{PARENT_ID}/local:Child"), "label"),
        Some(serde_json::json!("two"))
    );
    let tree = component.last_tree.as_ref().unwrap();
    assert!(node_with_content(tree, "two").is_some());
    assert!(node_with_content(tree, "sibling").is_some());
}

#[test]
fn nested_descendant_state_change_invalidates_enclosing_memo() {
    let mut component = memo_surface(
        r#"
<template>
    <box>
        <Child />
    </box>
</template>
<script lang="luau">
local Child = require("./components/child.mesh")
</script>
"#,
        &[
            (
                "Child",
                r#"
<template>
    <box>
        <GrandChild />
    </box>
</template>
<script lang="luau">
local GrandChild = require("./components/grand-child.mesh")
</script>
"#,
            ),
            (
                "GrandChild",
                r#"
<template>
    <text content="{value}" />
</template>
<script lang="luau">
value = 0
function bump()
    value = value + 1
end
</script>
"#,
            ),
        ],
    );

    let theme = default_theme();
    let mut buffer = PixelBuffer::new(160, 60);
    component.paint(&theme, 160, 60, &mut buffer, 1.0).unwrap();
    assert!(
        node_with_content(component.last_tree.as_ref().unwrap(), "0").is_some(),
        "initial grandchild value renders"
    );
    // Warm-up: settle container sizes and populate the memo, then prove the
    // untouched Child subtree is served from cache.
    rebuild(&mut component, 160, 60);
    let hits_before = component.component_memo_hit_count();
    rebuild(&mut component, 160, 60);
    assert_eq!(component.component_memo_hit_count(), hits_before + 1);
    let hits_before = component.component_memo_hit_count();

    // Mutating the grandchild's state must invalidate the enclosing Child
    // memo entry (descendant generation mismatch), not serve a stale subtree.
    component
        .call_namespaced_handler(
            &format!("__mesh_embed__::{PARENT_ID}/local:Child/local:GrandChild::bump"),
            &[],
        )
        .unwrap();
    component.paint(&theme, 160, 60, &mut buffer, 1.0).unwrap();
    assert_eq!(component.component_memo_hit_count(), hits_before);
    assert!(
        node_with_content(component.last_tree.as_ref().unwrap(), "1").is_some(),
        "rebuilt grandchild value renders through the enclosing component"
    );
}

#[test]
fn memoized_popover_wrapper_replays_promotion_flag() {
    let popover_src = r#"
<template>
    <popover>
        <text content="menu" />
    </popover>
</template>
"#;
    let mut parent_manifest = minimal_test_manifest(PARENT_ID);
    parent_manifest.dependencies.modules.insert(
        "@mesh/menu-popover".into(),
        mesh_core_module::manifest::DependencySpec::Simple(">=0.1.0".into()),
    );
    let mut popover_manifest = minimal_test_manifest("@mesh/menu-popover");
    popover_manifest.package.module_type = mesh_core_module::ModuleType::Component;

    let parent_compiled = CompiledFrontendModule {
        manifest: parent_manifest,
        source_path: PathBuf::from("src/main.mesh"),
        component: parse_component(
            r#"
<template>
    <box>
        <MenuPopover />
    </box>
</template>
<script lang="luau">
import MenuPopover from "@mesh/menu-popover"
</script>
"#,
        )
        .unwrap(),
        local_components: HashMap::new(),
        module_component_imports: HashMap::from([(
            "MenuPopover".into(),
            "@mesh/menu-popover".into(),
        )]),
        watched_paths: Vec::new(),
    };
    let popover_compiled = CompiledFrontendModule {
        manifest: popover_manifest,
        source_path: PathBuf::from("src/main.mesh"),
        component: parse_component(popover_src).unwrap(),
        local_components: HashMap::new(),
        module_component_imports: HashMap::new(),
        watched_paths: Vec::new(),
    };
    let catalog = FrontendCatalog {
        modules: HashMap::from([
            (
                PARENT_ID.into(),
                FrontendCatalogEntry {
                    module_dir: PathBuf::from("."),
                    compiled: parent_compiled.clone(),
                },
            ),
            (
                "@mesh/menu-popover".into(),
                FrontendCatalogEntry {
                    module_dir: PathBuf::from("."),
                    compiled: popover_compiled,
                },
            ),
        ]),
        slot_contributions: HashMap::new(),
    };
    let mut component = FrontendSurfaceComponent::new(
        parent_compiled,
        PathBuf::from("."),
        catalog,
        InterfaceCatalog::default(),
    );
    component
        .mount(ComponentContext {
            component_id: PARENT_ID.into(),
            surface_id: PARENT_ID.into(),
            diagnostics: Diagnostics::new(PARENT_ID),
        })
        .unwrap();
    component.visible = true;

    let theme = default_theme();
    let mut buffer = PixelBuffer::new(160, 60);
    component.paint(&theme, 160, 60, &mut buffer, 1.0).unwrap();
    assert!(component.has_promoted_popover_wrappers.get());
    // Warm-up: settle container sizes and populate the memo.
    rebuild(&mut component, 160, 60);
    let hits_before = component.component_memo_hit_count();

    // On a rebuild that serves the popover wrapper from the memo, the
    // promotion presence flag (reset at the top of each surface build) must be
    // replayed so finalize_tree still collapses the wrapper out of flow.
    component.invalidate_script_state();
    component.paint(&theme, 160, 60, &mut buffer, 1.0).unwrap();
    assert_eq!(component.component_memo_hit_count(), hits_before + 1);
    assert!(
        component.has_promoted_popover_wrappers.get(),
        "memo hit must replay the promoted-popover presence flag"
    );
    let tree = component.last_tree.as_ref().unwrap();
    let wrapper = first_node_by_tag(tree, "surface").or(Some(tree)).unwrap();
    assert!(
        find_marker(wrapper),
        "cached wrapper keeps the promoted-popover marker"
    );
}

fn find_marker(node: &WidgetNode) -> bool {
    if node
        .attributes
        .get(crate::shell::component::PROMOTED_POPOVER_MARKER)
        .is_some_and(|value| value == "true")
    {
        return true;
    }
    node.children.iter().any(find_marker)
}

// cargo test -p mesh-core-shell --release -- memoized_rebuild_beats_full_child_reeval --ignored --nocapture
#[test]
#[ignore = "release-only component render memoization benchmark"]
fn memoized_rebuild_beats_full_child_reeval() {
    let child_src = r#"
<template>
    <row>
        <icon name="{icon}" />
        <text content="{label}" />
        <text content="{detail}" />
    </row>
</template>
<script lang="luau">
icon = "info"
label = ""
detail = ""
function render(self)
    detail = string.format("%s!", label)
end
</script>
"#;
    // Distinct aliases: repeated same-alias instances share one runtime and
    // deliberately miss the memo (props ping-pong bumps the shared state), so
    // model a navigation-bar-style surface of distinct child components.
    let children: String = (0..12)
        .map(|index| format!("        <Item{index} label=\"row {index}\" />\n"))
        .collect();
    let imports: String = (0..12)
        .map(|index| format!("local Item{index} = require(\"./components/item-{index}.mesh\")\n"))
        .collect();
    let parent_src = format!(
        r#"
<template>
    <box>
{children}    </box>
</template>
<script lang="luau">
{imports}</script>
"#
    );
    let aliases: Vec<String> = (0..12).map(|index| format!("Item{index}")).collect();
    let locals: Vec<(&str, &str)> = aliases
        .iter()
        .map(|alias| (alias.as_str(), child_src))
        .collect();
    let mut component = memo_surface(&parent_src, &locals);

    let iterations = 200;
    let theme = default_theme();
    let mut buffer = PixelBuffer::new(400, 300);
    component.paint(&theme, 400, 300, &mut buffer, 1.0).unwrap();
    rebuild(&mut component, 400, 300);

    // Miss path: clear the memo before every rebuild so each child re-runs
    // template evaluation, prop sync, and style resolution.
    let miss_started = std::time::Instant::now();
    for _ in 0..iterations {
        component.clear_component_memo();
        rebuild(&mut component, 400, 300);
    }
    let miss_time = miss_started.elapsed();

    // Hit path: unchanged children served from the memo on every rebuild.
    rebuild(&mut component, 400, 300);
    let hits_before = component.component_memo_hit_count();
    let hit_started = std::time::Instant::now();
    for _ in 0..iterations {
        rebuild(&mut component, 400, 300);
    }
    let hit_time = hit_started.elapsed();
    let hits = component.component_memo_hit_count() - hits_before;

    eprintln!(
        "component memoization: forced-miss rebuilds {miss_time:?}; memoized rebuilds {hit_time:?}; ratio {:.1}x; hits={hits}",
        miss_time.as_secs_f64() / hit_time.as_secs_f64()
    );
    assert_eq!(hits, iterations * 12);
    assert!(hit_time < miss_time);
}
