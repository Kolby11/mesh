use super::super::luau_scan;
use mesh_core_component::{
    Attribute, AttributeValue, ComponentFile, SourceTag, TemplateNode, parse_component,
};
use std::path::Path;

fn extract_icon_names_from_component(component: &ComponentFile) -> Vec<String> {
    let mut names = Vec::new();
    let Some(template) = &component.template else {
        return names;
    };

    for node in &template.root {
        collect_icon_names_from_template_node(node, &mut names);
    }
    names.sort();
    names.dedup();
    names
}

fn collect_icon_names_from_template_node(node: &TemplateNode, names: &mut Vec<String>) {
    match node {
        TemplateNode::Element(element) => {
            if element.tag_kind == SourceTag::Icon {
                for attribute in &element.attributes {
                    if attribute.name == "name"
                        && let AttributeValue::Static(name) = &attribute.value
                        && !name.is_empty()
                    {
                        names.push(name.clone());
                    }
                }
            }
            for child in &element.children {
                collect_icon_names_from_template_node(child, names);
            }
        }
        TemplateNode::If(node) => {
            for child in &node.then_children {
                collect_icon_names_from_template_node(child, names);
            }
            for child in &node.else_children {
                collect_icon_names_from_template_node(child, names);
            }
        }
        TemplateNode::For(node) => {
            for child in &node.children {
                collect_icon_names_from_template_node(child, names);
            }
        }
        TemplateNode::Component(component) => {
            for child in &component.children {
                collect_icon_names_from_template_node(child, names);
            }
        }
        TemplateNode::Text(_) | TemplateNode::Expr(_) | TemplateNode::Slot(_) => {}
    }
}

/// Static call arguments a `.mesh` file's Luau supplies to the calls graph
/// scanning cross-checks.
#[derive(Debug, Default)]
pub(crate) struct MeshStaticCalls {
    /// Keys passed to `t("...")`.
    pub(crate) t_keys: Vec<String>,
    /// Channels passed to `mesh.events.publish("...")`.
    pub(crate) publish_channels: Vec<String>,
}

const MESH_SCANNED_CALLEES: [&str; 2] = ["t", "mesh.events.publish"];

/// Scan one `.mesh` file's Luau in a single parse pass.
///
/// Both halves of the file run Luau — the `<script>` chunk and every template
/// expression (`{t('nav.volume') .. suffix}`) — so both are collected and
/// handed to the parser together.
fn extract_mesh_static_calls_from_component(component: &ComponentFile) -> MeshStaticCalls {
    let mut sources = luau_scan::LuauSources::default();
    if let Some(script) = &component.script {
        sources.chunks.push(script.source.as_str());
    }
    if let Some(template) = &component.template {
        for node in &template.root {
            collect_luau_expressions_from_template_node(node, &mut sources.expressions);
        }
    }

    let mut found = luau_scan::static_call_string_arguments(&sources, &MESH_SCANNED_CALLEES);
    let publish_channels = found.pop().unwrap_or_default();
    let mut t_keys = found.pop().unwrap_or_default();
    if let Some(props) = &component.props {
        for prop in &props.props {
            for label in [&prop.label, &prop.description] {
                if let Some(mesh_core_component::LocalizedLabel::Translation { key, .. }) = label {
                    t_keys.push(key.clone());
                }
            }
        }
    }
    t_keys.sort();
    t_keys.dedup();
    MeshStaticCalls {
        t_keys,
        publish_channels,
    }
}

#[cfg(test)]
pub(crate) fn extract_t_keys_from_mesh_source(content: &str) -> Vec<String> {
    scan_mesh_source(content).static_calls.t_keys
}

#[cfg(test)]
pub(crate) fn extract_mesh_event_publish_channels(content: &str) -> Vec<String> {
    scan_mesh_source(content).static_calls.publish_channels
}

/// Interface events a backend `.luau` file emits through a provider-owned
/// `self.EventName:fire(...)` handle with a static name. Backend entrypoints are
/// plain Luau chunks, with no template half.
pub(crate) fn extract_backend_event_names(content: &str) -> Vec<String> {
    luau_scan::static_self_event_names_in_chunk(content)
}

fn collect_luau_expressions_from_template_node<'a>(
    node: &'a TemplateNode,
    expressions: &mut Vec<&'a str>,
) {
    match node {
        TemplateNode::Element(element) => {
            collect_luau_expressions_from_attributes(&element.attributes, expressions);
            for child in &element.children {
                collect_luau_expressions_from_template_node(child, expressions);
            }
        }
        TemplateNode::Component(component) => {
            collect_luau_expressions_from_attributes(&component.props, expressions);
            for child in &component.children {
                collect_luau_expressions_from_template_node(child, expressions);
            }
        }
        TemplateNode::Expr(expr) => expressions.push(expr.expression.as_str()),
        TemplateNode::If(node) => {
            expressions.push(node.condition.as_str());
            for child in node.then_children.iter().chain(&node.else_children) {
                collect_luau_expressions_from_template_node(child, expressions);
            }
        }
        TemplateNode::For(node) => {
            expressions.push(node.iterable.as_str());
            for child in &node.children {
                collect_luau_expressions_from_template_node(child, expressions);
            }
        }
        TemplateNode::Text(_) | TemplateNode::Slot(_) => {}
    }
}

fn collect_luau_expressions_from_attributes<'a>(
    attributes: &'a [Attribute],
    expressions: &mut Vec<&'a str>,
) {
    for attribute in attributes {
        match &attribute.value {
            AttributeValue::Binding(expression) => expressions.push(expression.as_str()),
            AttributeValue::EventHandlerCall { args, .. } => {
                expressions.extend(args.iter().map(String::as_str));
            }
            // A handler name, a bound variable name, and static text are not
            // expressions.
            AttributeValue::Static(_)
            | AttributeValue::TwoWayBinding(_)
            | AttributeValue::InstanceBinding(_)
            | AttributeValue::EventHandler(_) => {}
        }
    }
}

pub(crate) fn extract_frontend_interface_event_subscriptions(
    content: &str,
) -> Vec<(String, String)> {
    let Ok(component) = parse_component(content) else {
        return Vec::new();
    };
    let Some(script) = component.script else {
        return Vec::new();
    };
    let mut subscriptions = script.metadata.interface_event_subscriptions;
    subscriptions.sort();
    subscriptions.dedup();
    subscriptions
}

fn extract_keybind_subscriptions_from_component(component: &ComponentFile) -> Vec<(String, bool)> {
    let mut subscriptions = Vec::new();
    let Some(template) = &component.template else {
        return subscriptions;
    };

    for node in &template.root {
        collect_keybind_subscriptions_from_template_node(node, &mut subscriptions);
    }
    subscriptions.sort();
    subscriptions.dedup();
    subscriptions
}

#[derive(Debug, Default)]
pub(crate) struct MeshSourceScan {
    pub(crate) icon_names: Vec<String>,
    pub(crate) static_calls: MeshStaticCalls,
    pub(crate) keybind_subscriptions: Vec<(String, bool)>,
}

pub(crate) fn scan_mesh_source(content: &str) -> MeshSourceScan {
    let Ok(component) = parse_component(content) else {
        return MeshSourceScan::default();
    };
    MeshSourceScan {
        icon_names: extract_icon_names_from_component(&component),
        static_calls: extract_mesh_static_calls_from_component(&component),
        keybind_subscriptions: extract_keybind_subscriptions_from_component(&component),
    }
}

fn collect_keybind_subscriptions_from_template_node(
    node: &TemplateNode,
    subscriptions: &mut Vec<(String, bool)>,
) {
    match node {
        TemplateNode::Element(element) => {
            collect_keybind_subscription_from_attributes(&element.attributes, subscriptions);
            for child in &element.children {
                collect_keybind_subscriptions_from_template_node(child, subscriptions);
            }
        }
        TemplateNode::If(node) => {
            for child in &node.then_children {
                collect_keybind_subscriptions_from_template_node(child, subscriptions);
            }
            for child in &node.else_children {
                collect_keybind_subscriptions_from_template_node(child, subscriptions);
            }
        }
        TemplateNode::For(node) => {
            for child in &node.children {
                collect_keybind_subscriptions_from_template_node(child, subscriptions);
            }
        }
        TemplateNode::Component(component) => {
            collect_keybind_subscription_from_attributes(&component.props, subscriptions);
            for child in &component.children {
                collect_keybind_subscriptions_from_template_node(child, subscriptions);
            }
        }
        TemplateNode::Text(_) | TemplateNode::Expr(_) | TemplateNode::Slot(_) => {}
    }
}

fn collect_keybind_subscription_from_attributes(
    attributes: &[Attribute],
    subscriptions: &mut Vec<(String, bool)>,
) {
    let has_handler = attributes
        .iter()
        .any(|attribute| attribute.name == "onkeybind");
    let Some(attribute) = attributes
        .iter()
        .find(|attribute| attribute.name == "keybind")
    else {
        return;
    };

    let action_id = match &attribute.value {
        AttributeValue::Static(value) => {
            let value = value.trim();
            (!value.is_empty() && !value.contains(['{', '}', '.'])).then_some(value)
        }
        AttributeValue::Binding(expression) => expression
            .trim()
            .strip_prefix("this.keybinds.")
            .and_then(|action| action.strip_suffix(".id"))
            .filter(|action| !action.is_empty() && !action.contains('.')),
        _ => None,
    };
    if let Some(action_id) = action_id {
        subscriptions.push((action_id.to_string(), has_handler));
    }
}

pub(super) fn is_declared_shell_event_channel(channel: &str) -> bool {
    matches!(
        channel,
        "shell.show-surface"
            | "shell.hide-surface"
            | "shell.hide-popover"
            | "shell.toggle-surface"
            | "shell.position-surface"
            | "shell.activate-popover"
            | "shell.set-theme"
            | "shell.set-locale"
            | "shell.set-provider"
            | "shell.set-module-enabled"
            | "shell.set-module-prop"
            | "shell.unset-module-prop"
            | "shell.toggle-debug-overlay"
            | "shell.toggle-debug-layout-bounds"
            | "shell.toggle-debug-profiling"
            | "shell.run-debug-benchmark"
            | "shell.brightness-down"
            | "shell.brightness-up"
            | "shell.set-brightness"
            | "shell.toggle-calendar"
    )
}

pub(super) fn scan_mesh_files_recursive(dir: &Path) -> Vec<(std::path::PathBuf, String)> {
    scan_files_recursive(dir, "mesh")
}

pub(super) fn scan_files_recursive(
    dir: &Path,
    extension: &str,
) -> Vec<(std::path::PathBuf, String)> {
    let mut results = Vec::new();
    let Ok(root_metadata) = std::fs::symlink_metadata(dir) else {
        return results;
    };
    if root_metadata.file_type().is_symlink() || !root_metadata.is_dir() {
        return results;
    }
    let Ok(root) = std::fs::canonicalize(dir) else {
        return results;
    };
    let mut visited = std::collections::HashSet::new();
    scan_files_recursive_into(dir, &root, extension, &mut visited, &mut results);
    results
}

fn scan_files_recursive_into(
    dir: &Path,
    root: &Path,
    extension: &str,
    visited: &mut std::collections::HashSet<std::path::PathBuf>,
    results: &mut Vec<(std::path::PathBuf, String)>,
) {
    let Ok(canonical) = std::fs::canonicalize(dir) else {
        return;
    };
    if !canonical.starts_with(root) || !visited.insert(canonical) {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(metadata) = std::fs::symlink_metadata(&path) else {
            continue;
        };
        if metadata.file_type().is_symlink() {
            continue;
        }
        if metadata.is_dir() {
            scan_files_recursive_into(&path, root, extension, visited, results);
        } else if metadata.is_file()
            && path.extension().and_then(|e| e.to_str()) == Some(extension)
            && let Ok(content) = std::fs::read_to_string(&path)
        {
            results.push((path, content));
        }
    }
}
