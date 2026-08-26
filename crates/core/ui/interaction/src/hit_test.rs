#![allow(dead_code)] // Legacy hit-test helpers remain for interaction regression fixtures.

use super::*;
use mesh_core_elements::HandlerTarget;
use mesh_core_elements::style::TooltipAnchor;
use std::sync::Arc;
#[derive(Debug, Clone, PartialEq)]
pub struct PointerHit {
    pub path: Vec<String>,
    pub tooltip: Option<(String, String)>,
    pub bounds: ContentBounds,
}

#[derive(Debug, Clone, Copy)]
pub struct PointerPressNode<'a> {
    pub key: &'a str,
    pub node: &'a WidgetNode,
    pub bounds: ContentBounds,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct PointerPressHit<'a> {
    pub target: Option<PointerPressNode<'a>>,
    pub focusable: Option<PointerPressNode<'a>>,
}

#[derive(Debug, Clone, Copy)]
pub struct PointerEventHandlerHit<'a> {
    pub key: &'a str,
    pub node: &'a WidgetNode,
    pub bounds: ContentBounds,
}

/// Deepest visible node under a point, including non-interactive and synthetic
/// ones so the debug element picker can inspect everything painted.
#[derive(Debug, Clone, Copy)]
pub struct InspectHit<'a> {
    pub node: &'a WidgetNode,
    pub bounds: ContentBounds,
}

pub fn inspect_hit_test(node: &WidgetNode, x: f32, y: f32) -> Option<InspectHit<'_>> {
    inspect_hit_test_affine(
        node,
        x,
        y,
        root_transform(0.0, 0.0),
        &AffineClipStack::default(),
    )
}

fn inspect_hit_test_inner(
    node: &WidgetNode,
    x: f32,
    y: f32,
    offset_x: f32,
    offset_y: f32,
) -> Option<InspectHit<'_>> {
    if !node_allows(node, InteractionTarget::Paint) {
        return None;
    }
    let (offset_x, offset_y) = apply_transform_offset(node, offset_x, offset_y);
    let inside = layout_contains_with_offset(node, x, y, offset_x, offset_y);
    if !inside && node_clips_children(node) {
        return None;
    }
    let (child_x, child_y) = child_offsets_with_scroll(node, offset_x, offset_y);
    for child in node.children.iter().rev() {
        if let Some(hit) = inspect_hit_test_inner(child, x, y, child_x, child_y) {
            return Some(hit);
        }
    }
    inside.then(|| InspectHit {
        node,
        bounds: node_rect_with_offset(node, offset_x, offset_y),
    })
}

#[derive(Debug, Clone, Copy)]
pub struct TooltipTarget<'a> {
    pub owner: &'a WidgetNode,
    pub text: &'a str,
    pub bounds: ContentBounds,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TooltipRenderTarget {
    pub text: Arc<str>,
    pub bounds: Option<ContentBounds>,
    pub anchor: TooltipAnchor,
    pub offset: Option<(f32, f32)>,
}

#[derive(Clone, Debug)]
struct CachedTooltipTarget {
    generation: u64,
    hovered_key: String,
    target: TooltipRenderTarget,
}

#[derive(Clone, Debug, Default)]
pub struct TooltipTargetCache {
    cached: Option<CachedTooltipTarget>,
}

impl TooltipTargetCache {
    pub fn clear(&mut self) {
        self.cached = None;
    }

    pub fn resolve<'a>(
        &'a mut self,
        tree: &WidgetNode,
        hovered_key: &str,
        retained_generation: u64,
        fallback_bounds: Option<ContentBounds>,
    ) -> Option<&'a TooltipRenderTarget> {
        let is_current = self.cached.as_ref().is_some_and(|cached| {
            cached.generation == retained_generation && cached.hovered_key == hovered_key
        });
        if !is_current {
            let Some(tooltip) = find_tooltip_target_by_key(tree, hovered_key) else {
                self.cached = None;
                return None;
            };
            // Without a stable key, placement falls back to the pointer-hit
            // bounds and default style.
            let keyed_owner = tooltip.owner.mesh_key().map(|_| tooltip.owner);
            self.cached = Some(CachedTooltipTarget {
                generation: retained_generation,
                hovered_key: hovered_key.to_owned(),
                target: TooltipRenderTarget {
                    text: Arc::from(tooltip.text),
                    bounds: keyed_owner.map(|_| tooltip.bounds).or(fallback_bounds),
                    anchor: keyed_owner
                        .map(|node| node.computed_style.tooltip_anchor)
                        .unwrap_or_default(),
                    offset: keyed_owner.and_then(|node| node.computed_style.tooltip_offset),
                },
            });
        }
        self.cached.as_ref().map(|cached| &cached.target)
    }
}

impl TooltipTarget<'_> {
    fn into_owned(self) -> (String, String) {
        let owner = self
            .owner
            .mesh_key()
            .map(str::to_owned)
            .unwrap_or_else(|| format!("anonymous-tooltip-owner:{:p}", self.owner));
        (owner, self.text.to_owned())
    }
}

#[derive(Clone, Copy)]
struct TooltipRef<'a> {
    owner: &'a WidgetNode,
    text: &'a str,
}

impl TooltipRef<'_> {
    fn into_owned(self) -> (String, String) {
        let owner = self
            .owner
            .mesh_key()
            .map(str::to_owned)
            .unwrap_or_else(|| format!("anonymous-tooltip-owner:{:p}", self.owner));
        (owner, self.text.to_owned())
    }
}

/// Resolve all pointer-motion metadata in the same tree traversal.
pub fn pointer_hit_test(node: &WidgetNode, x: f32, y: f32) -> Option<PointerHit> {
    let mut hit = pointer_hit_test_reversed_affine(
        node,
        x,
        y,
        root_transform(0.0, 0.0),
        &AffineClipStack::default(),
        None,
    )?;
    hit.path.reverse();
    Some(hit)
}

/// Press targeting in one traversal: the deepest pointer-focusable node, or
/// failing that the deepest ancestor with a click handler, plus its bounds.
pub fn pointer_press_hit(node: &WidgetNode, x: f32, y: f32) -> PointerPressHit<'_> {
    let mut hit = pointer_press_hit_affine(
        node,
        x,
        y,
        root_transform(0.0, 0.0),
        &AffineClipStack::default(),
    )
    .unwrap_or_default();
    hit.target = hit.focusable.or(hit.target);
    hit
}

/// Deepest node under the pointer owning a plain event handler, without the
/// path allocation and per-key tree walks a two-pass search would need.
pub fn pointer_event_handler_hit<'a>(
    node: &'a WidgetNode,
    x: f32,
    y: f32,
    event_name: &str,
) -> Option<PointerEventHandlerHit<'a>> {
    pointer_event_handler_hit_affine(
        node,
        x,
        y,
        event_name,
        root_transform(0.0, 0.0),
        &AffineClipStack::default(),
    )
}

fn pointer_event_handler_hit_inner<'a>(
    node: &'a WidgetNode,
    x: f32,
    y: f32,
    event_name: &str,
    offset_x: f32,
    offset_y: f32,
) -> Option<PointerEventHandlerHit<'a>> {
    if !node_allows(node, InteractionTarget::Pointer) {
        return None;
    }

    let (offset_x, offset_y) = apply_transform_offset(node, offset_x, offset_y);
    let inside = layout_contains_with_offset(node, x, y, offset_x, offset_y);
    if !inside && node_clips_children(node) {
        return None;
    }

    let (child_offset_x, child_offset_y) = child_offsets_with_scroll(node, offset_x, offset_y);
    for child in node.children.iter().rev() {
        if let Some(hit) =
            pointer_event_handler_hit_inner(child, x, y, event_name, child_offset_x, child_offset_y)
        {
            return Some(hit);
        }
    }

    if inside && node.event_handlers.contains_key(event_name) {
        return node.mesh_key().map(|key| PointerEventHandlerHit {
            key,
            node,
            bounds: node_rect_with_offset(node, offset_x, offset_y),
        });
    }

    None
}

fn pointer_press_hit_inner<'a>(
    node: &'a WidgetNode,
    x: f32,
    y: f32,
    offset_x: f32,
    offset_y: f32,
) -> Option<PointerPressHit<'a>> {
    if !node_allows(node, InteractionTarget::Pointer) {
        return None;
    }

    let (offset_x, offset_y) = apply_transform_offset(node, offset_x, offset_y);
    let inside = layout_contains_with_offset(node, x, y, offset_x, offset_y);
    if !inside && node_clips_children(node) {
        return None;
    }

    let (child_offset_x, child_offset_y) = child_offsets_with_scroll(node, offset_x, offset_y);
    let mut hit = PointerPressHit::default();
    for child in node.children.iter().rev() {
        if let Some(child_hit) =
            pointer_press_hit_inner(child, x, y, child_offset_x, child_offset_y)
        {
            hit.focusable = child_hit.focusable;
            hit.target = child_hit.target;
            break;
        }
    }

    if inside && let Some(key) = node.mesh_key() {
        let node_hit = PointerPressNode {
            key,
            node,
            bounds: node_rect_with_offset(node, offset_x, offset_y),
        };
        if hit.focusable.is_none() && crate::focus::node_is_pointer_focusable(node) {
            hit.focusable = Some(node_hit);
        }
        if hit.target.is_none() && node.event_handlers.contains_key("click") {
            hit.target = Some(node_hit);
        }
    }

    (hit.focusable.is_some() || hit.target.is_some()).then_some(hit)
}

fn pointer_hit_test_reversed<'a>(
    node: &'a WidgetNode,
    x: f32,
    y: f32,
    offset_x: f32,
    offset_y: f32,
    inherited_tooltip: Option<TooltipTarget<'a>>,
) -> Option<PointerHit> {
    if !node_allows(node, InteractionTarget::Pointer) {
        return None;
    }
    let (offset_x, offset_y) = apply_transform_offset(node, offset_x, offset_y);
    let inside = layout_contains_with_offset(node, x, y, offset_x, offset_y);
    if !inside && node_clips_children(node) {
        return None;
    }
    let owner_tooltip = node_allows(node, InteractionTarget::Tooltip)
        .then(|| node_tooltip_text_ref(node))
        .flatten()
        .map(|text| TooltipTarget {
            owner: node,
            text,
            bounds: node_rect_with_offset(node, offset_x, offset_y),
        });
    let tooltip = owner_tooltip.or(inherited_tooltip);
    let (child_ox, child_oy) = child_offsets_with_scroll(node, offset_x, offset_y);
    for child in node.children.iter().rev() {
        if let Some(mut hit) = pointer_hit_test_reversed(child, x, y, child_ox, child_oy, tooltip) {
            if let Some(key) = node.mesh_key() {
                hit.path.push(key.to_owned());
            }
            return Some(hit);
        }
    }
    let key = node.mesh_key()?;
    inside.then(|| PointerHit {
        path: vec![key.to_owned()],
        tooltip: tooltip.map(TooltipTarget::into_owned),
        bounds: tooltip
            .map(|tooltip| tooltip.bounds)
            .unwrap_or_else(|| node_rect_with_offset(node, offset_x, offset_y)),
    })
}

pub fn find_node_by_key<'a>(node: &'a WidgetNode, key: &str) -> Option<&'a WidgetNode> {
    if node.mesh_key().is_some_and(|value| value == key) {
        return Some(node);
    }

    for child in &node.children {
        if let Some(found) = find_node_by_key(child, key) {
            return Some(found);
        }
    }

    None
}

pub fn source_element_tag(node: &WidgetNode) -> &str {
    node.attributes
        .get("data-mesh-element")
        .map(String::as_str)
        .unwrap_or(node.tag.as_str())
}

pub fn node_is_source(node: &WidgetNode, tags: &[&str]) -> bool {
    let source = source_element_tag(node);
    tags.iter().any(|tag| *tag == source)
}

/// Node references and bounds for a set of `_mesh_key`s in one traversal. Used
/// by hover-transition dispatch, where each key would otherwise re-walk.
pub fn find_nodes_by_keys<'a>(
    node: &'a WidgetNode,
    keys: &std::collections::HashSet<&str>,
) -> std::collections::HashMap<&'a str, (&'a WidgetNode, ContentBounds)> {
    let mut found = std::collections::HashMap::with_capacity(keys.len());
    collect_nodes_by_keys_affine(
        node,
        keys,
        root_transform(0.0, 0.0),
        &AffineClipStack::default(),
        &mut found,
    );
    found
}

fn collect_nodes_by_keys<'a>(
    node: &'a WidgetNode,
    keys: &std::collections::HashSet<&str>,
    offset_x: f32,
    offset_y: f32,
    found: &mut std::collections::HashMap<&'a str, (&'a WidgetNode, ContentBounds)>,
) {
    if found.len() == keys.len() || !node_allows(node, InteractionTarget::Pointer) {
        return;
    }
    let (offset_x, offset_y) = apply_transform_offset(node, offset_x, offset_y);
    if let Some(key) = node.mesh_key()
        && keys.contains(key)
    {
        found.insert(key, (node, node_rect_with_offset(node, offset_x, offset_y)));
    }
    let (child_offset_x, child_offset_y) = child_offsets_with_scroll(node, offset_x, offset_y);
    for child in &node.children {
        collect_nodes_by_keys(child, keys, child_offset_x, child_offset_y, found);
        if found.len() == keys.len() {
            break;
        }
    }
}

pub fn find_node_bounds_by_key(
    node: &WidgetNode,
    key: &str,
    offset_x: f32,
    offset_y: f32,
) -> Option<ContentBounds> {
    find_node_bounds_by_key_affine(
        node,
        key,
        root_transform(offset_x, offset_y),
        &AffineClipStack::default(),
    )
}

/// Allocation-free counterpart to [`find_nodes_by_keys`] for callers needing
/// exactly one node, such as slider drags.
pub fn find_node_with_bounds_by_key<'a>(
    node: &'a WidgetNode,
    key: &str,
) -> Option<(&'a WidgetNode, ContentBounds)> {
    find_node_with_bounds_by_key_affine(
        node,
        key,
        root_transform(0.0, 0.0),
        &AffineClipStack::default(),
    )
    .map(|(node, bounds)| (node, bounds))
}

/// Resolve a keyboard target's bounds, including descendants of a promoted
/// popover. Those wrappers are hidden and collapsed in their source surface
/// because the child surface owns their pixels, but the retained descendants
/// remain the authoritative source for keyboard dispatch and event payloads.
pub fn find_focus_node_with_bounds_by_key<'a>(
    node: &'a WidgetNode,
    key: &str,
) -> Option<(&'a WidgetNode, ContentBounds)> {
    find_focus_node_with_bounds_by_key_affine(
        node,
        key,
        root_transform(0.0, 0.0),
        &AffineClipStack::default(),
        false,
    )
}

fn find_focus_node_with_bounds_by_key_affine<'a>(
    node: &'a WidgetNode,
    key: &str,
    parent_transform: AffineTransform,
    clips: &AffineClipStack,
    promoted_ancestor: bool,
) -> Option<(&'a WidgetNode, ContentBounds)> {
    let promoted_wrapper = node.is_promoted_popover();
    let promoted = promoted_ancestor || promoted_wrapper;
    if !promoted_wrapper && !node_allows(node, InteractionTarget::Focus) {
        return None;
    }
    let world = node_world_transform(parent_transform, node);
    if node.mesh_key().is_some_and(|value| value == key) {
        return promoted
            .then(|| clipped_node_bounds(node, world, clips))
            .flatten()
            .map(|bounds| (node, bounds));
    }
    let child_world = child_world_transform(world, node);
    let child_clips = push_node_clip(clips, node, world);
    node.children.iter().find_map(|child| {
        find_focus_node_with_bounds_by_key_affine(child, key, child_world, &child_clips, promoted)
    })
}

fn find_node_with_bounds_by_key_at<'a>(
    node: &'a WidgetNode,
    key: &str,
    offset_x: f32,
    offset_y: f32,
) -> Option<(&'a WidgetNode, ContentBounds)> {
    if !node_allows(node, InteractionTarget::Pointer) {
        return None;
    }
    let (offset_x, offset_y) = apply_transform_offset(node, offset_x, offset_y);
    if node.mesh_key().is_some_and(|value| value == key) {
        return Some((node, node_rect_with_offset(node, offset_x, offset_y)));
    }

    let (child_offset_x, child_offset_y) = child_offsets_with_scroll(node, offset_x, offset_y);
    for child in &node.children {
        if let Some(found) =
            find_node_with_bounds_by_key_at(child, key, child_offset_x, child_offset_y)
        {
            return Some(found);
        }
    }
    None
}

/// Return the root-to-deepest key path under the cursor, regardless of type.
pub fn find_node_path_at(node: &WidgetNode, x: f32, y: f32) -> Option<Vec<String>> {
    let mut path = find_node_path_reversed_affine(
        node,
        x,
        y,
        root_transform(0.0, 0.0),
        &AffineClipStack::default(),
    )?;
    path.reverse();
    Some(path)
}

fn find_node_path_at_offset(
    node: &WidgetNode,
    x: f32,
    y: f32,
    offset_x: f32,
    offset_y: f32,
) -> Option<Vec<String>> {
    let mut reversed = find_node_path_reversed(node, x, y, offset_x, offset_y)?;
    reversed.reverse();
    Some(reversed)
}

/// Deepest-first; the caller reverses once, avoiding an insert-at-front per
/// ancestor.
fn find_node_path_reversed(
    node: &WidgetNode,
    x: f32,
    y: f32,
    offset_x: f32,
    offset_y: f32,
) -> Option<Vec<String>> {
    if !node_allows(node, InteractionTarget::Pointer) {
        return None;
    }

    let (offset_x, offset_y) = apply_transform_offset(node, offset_x, offset_y);
    let inside = layout_contains_with_offset(node, x, y, offset_x, offset_y);
    if !inside && node_clips_children(node) {
        return None;
    }

    let (child_ox, child_oy) = child_offsets_with_scroll(node, offset_x, offset_y);
    for child in node.children.iter().rev() {
        if let Some(mut path) = find_node_path_reversed(child, x, y, child_ox, child_oy) {
            if let Some(key) = node.mesh_key() {
                path.push(key.to_owned());
            }
            return Some(path);
        }
    }

    if inside {
        return node.mesh_key().map(|key| vec![key.to_owned()]);
    }

    None
}

/// Extract tooltip text from a node's attributes and accessibility metadata.
#[cfg(test)]
fn node_tooltip_text(node: &WidgetNode) -> Option<String> {
    node_tooltip_text_ref(node).map(str::to_owned)
}

fn node_tooltip_text_ref(node: &WidgetNode) -> Option<&str> {
    if node
        .attributes
        .get("data-tooltip-disabled")
        .is_some_and(|value| value == "true" || value == "1")
    {
        return None;
    }

    // `title`/`tooltip` are the dedicated, always-honored tooltip hints
    // (docs/spec/09-accessibility.md: "title ... tooltip + AT description") —
    // an author who wrote one meant it to show, regardless of what else the
    // node renders.
    for key in ["title", "tooltip"] {
        if let Some(value) = non_empty_tooltip_text(node.attributes.get(key).map(String::as_str)) {
            return Some(value);
        }
    }

    // `aria-label`/`description`/`aria-description` and the computed
    // accessibility label are accessible-*name* sources, not tooltip hints —
    // spec: "aria-label ... AT name when visible text isn't it". A control
    // that already renders its own visible text (any node with children, or
    // a container's aggregated accessibility label) doesn't need a
    // redundant tooltip repeating what's already on screen; only surface
    // these for leaf nodes that render no visible text of their own, such
    // as icon-only controls.
    if !node.children.is_empty() {
        return None;
    }

    for key in ["aria-label", "description", "aria-description"] {
        if let Some(value) = non_empty_tooltip_text(node.attributes.get(key).map(String::as_str)) {
            return Some(value);
        }
    }

    non_empty_tooltip_text(node.accessibility.label.as_deref())
        .or_else(|| non_empty_tooltip_text(node.accessibility.description.as_deref()))
}

fn non_empty_tooltip_text(value: Option<&str>) -> Option<&str> {
    value.filter(|value| !value.trim().is_empty())
}

/// Surface-space bounds of the innermost clipping ancestor, which tooltip
/// auto-placement treats as the box to stay inside — a clipping container is
/// the region the user perceives the element to live in. `None` when the key is
/// absent or nothing clips; the node is never its own container.
pub fn find_tooltip_container_bounds(node: &WidgetNode, key: &str) -> Option<ContentBounds> {
    find_container_bounds_affine(
        node,
        key,
        root_transform(0.0, 0.0),
        &AffineClipStack::default(),
        None,
    )
    .flatten()
}

/// Outer `Option` = keyed node found; inner = nearest clipping ancestor bounds.
fn find_container_bounds_inner(
    node: &WidgetNode,
    key: &str,
    offset_x: f32,
    offset_y: f32,
    nearest_clip: Option<ContentBounds>,
) -> Option<Option<ContentBounds>> {
    if !node_allows(node, InteractionTarget::Tooltip) {
        return None;
    }
    let (offset_x, offset_y) = apply_transform_offset(node, offset_x, offset_y);
    if node.mesh_key().is_some_and(|k| k == key) {
        return Some(nearest_clip);
    }
    let clips = node.computed_style.overflow_x.clips_contents()
        || node.computed_style.overflow_y.clips_contents();
    let nearest_clip = if clips {
        Some(node_rect_with_offset(node, offset_x, offset_y))
    } else {
        nearest_clip
    };
    let (child_offset_x, child_offset_y) = child_offsets_with_scroll(node, offset_x, offset_y);
    for child in &node.children {
        if let Some(found) =
            find_container_bounds_inner(child, key, child_offset_x, child_offset_y, nearest_clip)
        {
            return Some(found);
        }
    }
    None
}

/// Find tooltip text for a specific node key in the tree.
pub fn find_tooltip_text_by_key(node: &WidgetNode, key: &str) -> Option<String> {
    find_tooltip_by_key_with_inherited(node, key, None)
        .flatten()
        .map(|tooltip| tooltip.text.to_owned())
}

/// Find the tooltip owner key and text for a specific node key in the tree.
pub fn find_tooltip_by_key(node: &WidgetNode, key: &str) -> Option<(String, String)> {
    find_tooltip_by_key_with_inherited(node, key, None)
        .flatten()
        .map(TooltipRef::into_owned)
}

fn find_tooltip_by_key_with_inherited<'a>(
    node: &'a WidgetNode,
    key: &str,
    inherited: Option<TooltipRef<'a>>,
) -> Option<Option<TooltipRef<'a>>> {
    if !node_allows(node, InteractionTarget::Tooltip) {
        return None;
    }
    let owner_tooltip = node_tooltip_text_ref(node).map(|text| TooltipRef { owner: node, text });
    let inherited = owner_tooltip.or(inherited);
    if node.mesh_key().is_some_and(|candidate| candidate == key) {
        return Some(inherited);
    }
    for child in &node.children {
        if let Some(found) = find_tooltip_by_key_with_inherited(child, key, inherited) {
            return Some(found);
        }
    }
    None
}

/// Resolve inherited tooltip text, its owning node, and the owner's
/// surface-space bounds in one allocation-free traversal.
pub fn find_tooltip_target_by_key<'a>(
    node: &'a WidgetNode,
    key: &str,
) -> Option<TooltipTarget<'a>> {
    find_tooltip_target_by_key_affine(
        node,
        key,
        root_transform(0.0, 0.0),
        &AffineClipStack::default(),
        None,
    )
    .flatten()
}

/// Outer `Option` = keyed node found; inner = its nearest tooltip owner.
fn find_tooltip_target_by_key_inner<'a>(
    node: &'a WidgetNode,
    key: &str,
    offset_x: f32,
    offset_y: f32,
    inherited: Option<TooltipTarget<'a>>,
) -> Option<Option<TooltipTarget<'a>>> {
    if !node_allows(node, InteractionTarget::Tooltip) {
        return None;
    }
    let (offset_x, offset_y) = apply_transform_offset(node, offset_x, offset_y);
    let owner_tooltip = node_tooltip_text_ref(node).map(|text| TooltipTarget {
        owner: node,
        text,
        bounds: node_rect_with_offset(node, offset_x, offset_y),
    });
    let inherited = owner_tooltip.or(inherited);
    if node.mesh_key().is_some_and(|k| k == key) {
        return Some(inherited);
    }
    let (child_offset_x, child_offset_y) = child_offsets_with_scroll(node, offset_x, offset_y);
    for child in &node.children {
        if let Some(text) =
            find_tooltip_target_by_key_inner(child, key, child_offset_x, child_offset_y, inherited)
        {
            return Some(text);
        }
    }
    None
}

pub fn is_input_key(tree: &WidgetNode, key: &str) -> bool {
    find_node_by_key(tree, key).is_some_and(|node| {
        node.tag == "input"
            && node_is_source(
                node,
                &[
                    "input",
                    "textarea",
                    "search",
                    "password",
                    "number-input",
                    "stepper",
                    "text-input",
                    "password-input",
                    "search-input",
                    "email-input",
                    "url-input",
                ],
            )
    })
}

pub fn is_slider_key(tree: &WidgetNode, key: &str) -> bool {
    find_node_by_key(tree, key).is_some_and(|node| node.tag == "slider")
}

pub fn find_click_handler(tree: &WidgetNode, key: &str) -> Option<HandlerTarget> {
    find_event_handler(tree, key, "click")
}

pub fn find_event_handler(tree: &WidgetNode, key: &str, event_name: &str) -> Option<HandlerTarget> {
    let target = match event_name {
        "scroll" => InteractionTarget::Scroll,
        "click" => InteractionTarget::Pointer,
        _ => InteractionTarget::Focus,
    };
    find_node_by_key(tree, key)
        .filter(|node| node_allows(node, target))
        .and_then(|node| node.event_handlers.get(event_name))
        .cloned()
}

pub fn namespace_event_handlers(node: &mut WidgetNode, instance_key: &str) {
    for handler in node.event_handlers.values_mut() {
        handler.namespace(instance_key);
    }
    for call in node.event_handler_calls.values_mut() {
        call.handler.namespace(instance_key);
    }

    for child in &mut node.children {
        namespace_event_handlers(child, instance_key);
    }
}

fn clipped_node_bounds(
    node: &WidgetNode,
    world: AffineTransform,
    clips: &AffineClipStack,
) -> Option<ContentBounds> {
    let bounds = node_rect_with_transform(node, world);
    if clips.is_empty() {
        Some(bounds)
    } else {
        clips
            .bounds()
            .and_then(|clip| intersect_bounds(bounds, content_bounds_from_rect(clip)))
    }
}

fn content_bounds_from_rect(rect: mesh_core_elements::LayoutRect) -> ContentBounds {
    (rect.x, rect.y, rect.x + rect.width, rect.y + rect.height)
}

fn inspect_hit_test_affine<'a>(
    node: &'a WidgetNode,
    x: f32,
    y: f32,
    parent_transform: AffineTransform,
    clips: &AffineClipStack,
) -> Option<InspectHit<'a>> {
    if !node_allows(node, InteractionTarget::Paint) || !clips.contains(x, y) {
        return None;
    }
    let world = node_world_transform(parent_transform, node);
    let inside = node_contains_with_transform(node, world, x, y);
    if !inside && node_clips_children(node) {
        return None;
    }
    let child_world = child_world_transform(world, node);
    let child_clips = push_node_clip(clips, node, world);
    for child in node.children.iter().rev() {
        if let Some(hit) = inspect_hit_test_affine(child, x, y, child_world, &child_clips) {
            return Some(hit);
        }
    }
    inside.then(|| InspectHit {
        node,
        bounds: clipped_node_bounds(node, world, clips)
            .map(content_bounds)
            .unwrap_or_default(),
    })
}

fn content_bounds(bounds: ContentBounds) -> ContentBounds {
    bounds
}

fn pointer_event_handler_hit_affine<'a>(
    node: &'a WidgetNode,
    x: f32,
    y: f32,
    event_name: &str,
    parent_transform: AffineTransform,
    clips: &AffineClipStack,
) -> Option<PointerEventHandlerHit<'a>> {
    if !node_allows(node, InteractionTarget::Pointer) || !clips.contains(x, y) {
        return None;
    }
    let world = node_world_transform(parent_transform, node);
    let inside = node_contains_with_transform(node, world, x, y);
    if !inside && node_clips_children(node) {
        return None;
    }
    let child_world = child_world_transform(world, node);
    let child_clips = push_node_clip(clips, node, world);
    for child in node.children.iter().rev() {
        if let Some(hit) =
            pointer_event_handler_hit_affine(child, x, y, event_name, child_world, &child_clips)
        {
            return Some(hit);
        }
    }
    if inside && node.event_handlers.contains_key(event_name) {
        return node
            .mesh_key()
            .zip(clipped_node_bounds(node, world, clips))
            .map(|(key, bounds)| PointerEventHandlerHit { key, node, bounds });
    }
    None
}

fn pointer_press_hit_affine<'a>(
    node: &'a WidgetNode,
    x: f32,
    y: f32,
    parent_transform: AffineTransform,
    clips: &AffineClipStack,
) -> Option<PointerPressHit<'a>> {
    if !node_allows(node, InteractionTarget::Pointer) || !clips.contains(x, y) {
        return None;
    }
    let world = node_world_transform(parent_transform, node);
    let inside = node_contains_with_transform(node, world, x, y);
    if !inside && node_clips_children(node) {
        return None;
    }
    let child_world = child_world_transform(world, node);
    let child_clips = push_node_clip(clips, node, world);
    let mut hit = PointerPressHit::default();
    for child in node.children.iter().rev() {
        if let Some(child_hit) = pointer_press_hit_affine(child, x, y, child_world, &child_clips) {
            hit = child_hit;
            break;
        }
    }
    if inside
        && let Some((key, bounds)) = node.mesh_key().zip(clipped_node_bounds(node, world, clips))
    {
        let node_hit = PointerPressNode { key, node, bounds };
        if hit.focusable.is_none() && crate::focus::node_is_pointer_focusable(node) {
            hit.focusable = Some(node_hit);
        }
        if hit.target.is_none() && node.event_handlers.contains_key("click") {
            hit.target = Some(node_hit);
        }
    }
    (hit.focusable.is_some() || hit.target.is_some()).then_some(hit)
}

fn pointer_hit_test_reversed_affine<'a>(
    node: &'a WidgetNode,
    x: f32,
    y: f32,
    parent_transform: AffineTransform,
    clips: &AffineClipStack,
    inherited_tooltip: Option<TooltipTarget<'a>>,
) -> Option<PointerHit> {
    if !node_allows(node, InteractionTarget::Pointer) || !clips.contains(x, y) {
        return None;
    }
    let world = node_world_transform(parent_transform, node);
    let inside = node_contains_with_transform(node, world, x, y);
    if !inside && node_clips_children(node) {
        return None;
    }
    let owner_tooltip = node_allows(node, InteractionTarget::Tooltip)
        .then(|| node_tooltip_text_ref(node))
        .flatten()
        .and_then(|text| {
            clipped_node_bounds(node, world, clips).map(|bounds| TooltipTarget {
                owner: node,
                text,
                bounds,
            })
        });
    let tooltip = owner_tooltip.or(inherited_tooltip);
    let child_world = child_world_transform(world, node);
    let child_clips = push_node_clip(clips, node, world);
    for child in node.children.iter().rev() {
        if let Some(mut hit) =
            pointer_hit_test_reversed_affine(child, x, y, child_world, &child_clips, tooltip)
        {
            if let Some(key) = node.mesh_key() {
                hit.path.push(key.to_owned());
            }
            return Some(hit);
        }
    }
    let key = node.mesh_key()?;
    inside.then(|| PointerHit {
        path: vec![key.to_owned()],
        tooltip: tooltip.map(TooltipTarget::into_owned),
        bounds: tooltip
            .map(|tooltip| tooltip.bounds)
            .or_else(|| clipped_node_bounds(node, world, clips))
            .unwrap_or_default(),
    })
}

fn collect_nodes_by_keys_affine<'a>(
    node: &'a WidgetNode,
    keys: &std::collections::HashSet<&str>,
    parent_transform: AffineTransform,
    clips: &AffineClipStack,
    found: &mut std::collections::HashMap<&'a str, (&'a WidgetNode, ContentBounds)>,
) {
    if found.len() == keys.len() || !node_allows(node, InteractionTarget::Pointer) {
        return;
    }
    let world = node_world_transform(parent_transform, node);
    if let Some(key) = node.mesh_key()
        && keys.contains(key)
        && let Some(bounds) = clipped_node_bounds(node, world, clips)
    {
        found.insert(key, (node, bounds));
    }
    let child_world = child_world_transform(world, node);
    let child_clips = push_node_clip(clips, node, world);
    for child in &node.children {
        collect_nodes_by_keys_affine(child, keys, child_world, &child_clips, found);
        if found.len() == keys.len() {
            break;
        }
    }
}

fn find_node_bounds_by_key_affine<'a>(
    node: &'a WidgetNode,
    key: &str,
    parent_transform: AffineTransform,
    clips: &AffineClipStack,
) -> Option<ContentBounds> {
    if !node_allows(node, InteractionTarget::Pointer) {
        return None;
    }
    let world = node_world_transform(parent_transform, node);
    if node.mesh_key().is_some_and(|value| value == key) {
        return clipped_node_bounds(node, world, clips);
    }
    let child_world = child_world_transform(world, node);
    let child_clips = push_node_clip(clips, node, world);
    node.children
        .iter()
        .find_map(|child| find_node_bounds_by_key_affine(child, key, child_world, &child_clips))
}

fn find_node_with_bounds_by_key_affine<'a>(
    node: &'a WidgetNode,
    key: &str,
    parent_transform: AffineTransform,
    clips: &AffineClipStack,
) -> Option<(&'a WidgetNode, ContentBounds)> {
    if !node_allows(node, InteractionTarget::Pointer) {
        return None;
    }
    let world = node_world_transform(parent_transform, node);
    if node.mesh_key().is_some_and(|value| value == key) {
        return clipped_node_bounds(node, world, clips).map(|bounds| (node, bounds));
    }
    let child_world = child_world_transform(world, node);
    let child_clips = push_node_clip(clips, node, world);
    node.children.iter().find_map(|child| {
        find_node_with_bounds_by_key_affine(child, key, child_world, &child_clips)
    })
}

fn find_node_path_reversed_affine(
    node: &WidgetNode,
    x: f32,
    y: f32,
    parent_transform: AffineTransform,
    clips: &AffineClipStack,
) -> Option<Vec<String>> {
    if !node_allows(node, InteractionTarget::Pointer) || !clips.contains(x, y) {
        return None;
    }
    let world = node_world_transform(parent_transform, node);
    let inside = node_contains_with_transform(node, world, x, y);
    if !inside && node_clips_children(node) {
        return None;
    }
    let child_world = child_world_transform(world, node);
    let child_clips = push_node_clip(clips, node, world);
    for child in node.children.iter().rev() {
        if let Some(mut path) =
            find_node_path_reversed_affine(child, x, y, child_world, &child_clips)
        {
            if let Some(key) = node.mesh_key() {
                path.push(key.to_owned());
            }
            return Some(path);
        }
    }
    inside
        .then(|| node.mesh_key().map(|key| vec![key.to_owned()]))
        .flatten()
}

fn find_container_bounds_affine(
    node: &WidgetNode,
    key: &str,
    parent_transform: AffineTransform,
    clips: &AffineClipStack,
    nearest_clip: Option<ContentBounds>,
) -> Option<Option<ContentBounds>> {
    if !node_allows(node, InteractionTarget::Tooltip) {
        return None;
    }
    let world = node_world_transform(parent_transform, node);
    if node.mesh_key().is_some_and(|value| value == key) {
        return Some(nearest_clip);
    }
    let nearest_clip = if node_clips_children(node) {
        clipped_node_bounds(node, world, clips).or(nearest_clip)
    } else {
        nearest_clip
    };
    let child_world = child_world_transform(world, node);
    let child_clips = push_node_clip(clips, node, world);
    node.children.iter().find_map(|child| {
        find_container_bounds_affine(child, key, child_world, &child_clips, nearest_clip)
    })
}

fn find_tooltip_target_by_key_affine<'a>(
    node: &'a WidgetNode,
    key: &str,
    parent_transform: AffineTransform,
    clips: &AffineClipStack,
    inherited: Option<TooltipTarget<'a>>,
) -> Option<Option<TooltipTarget<'a>>> {
    if !node_allows(node, InteractionTarget::Tooltip) {
        return None;
    }
    let world = node_world_transform(parent_transform, node);
    let owner_tooltip = node_tooltip_text_ref(node).and_then(|text| {
        clipped_node_bounds(node, world, clips).map(|bounds| TooltipTarget {
            owner: node,
            text,
            bounds,
        })
    });
    let inherited = owner_tooltip.or(inherited);
    if node.mesh_key().is_some_and(|value| value == key) {
        return Some(inherited);
    }
    let child_world = child_world_transform(world, node);
    let child_clips = push_node_clip(clips, node, world);
    node.children.iter().find_map(|child| {
        find_tooltip_target_by_key_affine(child, key, child_world, &child_clips, inherited)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use mesh_core_elements::{EventHandlerCall, LayoutRect, WidgetNode};

    fn indexed_tree(rows: usize, columns: usize) -> WidgetNode {
        let mut root = WidgetNode::new("surface");
        root.attributes.insert("_mesh_key".into(), "root".into());
        root.layout = LayoutRect {
            x: 0.0,
            y: 0.0,
            width: columns as f32 * 20.0,
            height: rows as f32 * 20.0,
        };
        for row_index in 0..rows {
            let mut row = WidgetNode::new("row");
            row.attributes
                .insert("_mesh_key".into(), format!("row-{row_index}"));
            row.layout = LayoutRect {
                x: 0.0,
                y: row_index as f32 * 20.0,
                width: columns as f32 * 20.0,
                height: 20.0,
            };
            for column_index in 0..columns {
                let mut cell = WidgetNode::new("button");
                cell.attributes.insert(
                    "_mesh_key".into(),
                    format!("cell-{row_index}-{column_index}"),
                );
                cell.layout = LayoutRect {
                    x: column_index as f32 * 20.0,
                    y: 0.0,
                    width: 20.0,
                    height: 20.0,
                };
                row.children.push(cell);
            }
            root.children.push(row);
        }
        root
    }

    fn representative_handler_tree(rows: usize, columns: usize) -> WidgetNode {
        let mut tree = indexed_tree(rows, columns);
        for row in &mut tree.children {
            for cell in &mut row.children {
                cell.event_handlers
                    .insert("click".into(), "handlePrimaryAction".into());
                cell.event_handlers
                    .insert("pointerenter".into(), "handlePointerEnter".into());
                cell.event_handlers.insert(
                    "focus".into(),
                    HandlerTarget::embedded("@mesh/shared", "alreadyNamespaced"),
                );
                cell.event_handler_calls.insert(
                    "change".into(),
                    EventHandlerCall {
                        handler: "handleValueChange".into(),
                        args: Vec::new(),
                    },
                );
            }
        }
        tree
    }

    #[test]
    fn namespace_event_handlers_assigns_typed_owners_once() {
        let instance_key = "@mesh/settings/local:appearance/import:ThemeControls";
        let mut tree = representative_handler_tree(3, 4);
        namespace_event_handlers(&mut tree, instance_key);
        namespace_event_handlers(&mut tree, "ignored-second-owner");

        let cell = &tree.children[0].children[0];
        assert_eq!(
            cell.event_handlers["click"].handler(),
            "handlePrimaryAction"
        );
        assert_eq!(
            cell.event_handlers["click"].instance_key(),
            Some(instance_key)
        );
        assert_eq!(
            cell.event_handlers["focus"].instance_key(),
            Some("@mesh/shared")
        );
        assert_eq!(
            cell.event_handler_calls["change"].handler.instance_key(),
            Some(instance_key)
        );
    }

    #[test]
    fn pointer_hit_test_matches_separate_tree_walks() {
        let mut root = indexed_tree(6, 8);
        root.children[2]
            .attributes
            .insert("tooltip".into(), "Third row".into());
        root.children[2]
            .attributes
            .insert("_mesh_scroll_x".into(), "5".into());
        root.children[4].computed_style.transform.translate_x = 7.0;
        for y in (0..120).step_by(3) {
            for x in (0..160).step_by(3) {
                let fused = pointer_hit_test(&root, x as f32, y as f32);
                assert_eq!(
                    fused.as_ref().map(|hit| hit.path.clone()),
                    find_node_path_at(&root, x as f32, y as f32),
                    "mismatch at ({x}, {y})"
                );
            }
        }
        let hit = pointer_hit_test(&root, 48.0, 50.0).unwrap();
        assert_eq!(
            hit.bounds,
            find_node_bounds_by_key(&root, "row-2", 0.0, 0.0).unwrap()
        );
        assert_eq!(
            hit.tooltip,
            find_tooltip_by_key(&root, hit.path.last().unwrap())
        );
    }

    #[test]
    fn pointer_press_hit_matches_focusable_target_and_bounds() {
        let mut root = WidgetNode::new("surface");
        root.attributes.insert("_mesh_key".into(), "root".into());
        root.layout = LayoutRect {
            x: 0.0,
            y: 0.0,
            width: 120.0,
            height: 80.0,
        };
        let mut button = WidgetNode::new("button");
        button
            .attributes
            .insert("_mesh_key".into(), "button".into());
        button.layout = LayoutRect {
            x: 24.0,
            y: 20.0,
            width: 40.0,
            height: 20.0,
        };
        root.children.push(button);

        let hit = pointer_press_hit(&root, 48.0, 30.0);

        let target = hit.target.expect("focusable cell target");
        let focusable = hit.focusable.expect("focusable cell");
        assert_eq!(target.key, "button");
        assert_eq!(focusable.key, "button");
        assert_eq!(
            Some(target.key.to_owned()),
            crate::focus::find_focusable_at(&root, 48.0, 30.0)
        );
        assert_eq!(
            target.bounds,
            find_node_bounds_by_key(&root, "button", 0.0, 0.0).unwrap()
        );
    }

    #[test]
    fn pointer_press_hit_uses_clickable_ancestor_when_no_focusable_node_matches() {
        let mut root = WidgetNode::new("surface");
        root.attributes.insert("_mesh_key".into(), "root".into());
        root.layout = LayoutRect {
            x: 0.0,
            y: 0.0,
            width: 100.0,
            height: 40.0,
        };
        let mut clickable = WidgetNode::new("box");
        clickable
            .attributes
            .insert("_mesh_key".into(), "clickable".into());
        clickable
            .event_handlers
            .insert("click".into(), "onClick".into());
        clickable.layout = LayoutRect {
            x: 10.0,
            y: 5.0,
            width: 60.0,
            height: 25.0,
        };
        let mut label = WidgetNode::new("label");
        label.attributes.insert("_mesh_key".into(), "label".into());
        label.layout = LayoutRect {
            x: 12.0,
            y: 7.0,
            width: 20.0,
            height: 10.0,
        };
        clickable.children.push(label);
        root.children.push(clickable);

        let hit = pointer_press_hit(&root, 15.0, 10.0);

        assert!(hit.focusable.is_none());
        let target = hit.target.expect("clickable ancestor target");
        assert_eq!(target.key, "clickable");
        assert_eq!(
            target.bounds,
            find_node_bounds_by_key(&root, "clickable", 0.0, 0.0).unwrap()
        );
    }

    #[test]
    fn pointer_event_handler_hit_matches_nearest_handler_ancestor() {
        let mut root = WidgetNode::new("surface");
        root.attributes.insert("_mesh_key".into(), "root".into());
        root.layout = LayoutRect {
            x: 0.0,
            y: 0.0,
            width: 120.0,
            height: 80.0,
        };
        let mut scroll_owner = WidgetNode::new("box");
        scroll_owner
            .attributes
            .insert("_mesh_key".into(), "scroll-owner".into());
        scroll_owner
            .event_handlers
            .insert("scroll".into(), "onScroll".into());
        scroll_owner.layout = LayoutRect {
            x: 10.0,
            y: 10.0,
            width: 80.0,
            height: 40.0,
        };
        let mut label = WidgetNode::new("label");
        label.attributes.insert("_mesh_key".into(), "label".into());
        label.layout = LayoutRect {
            x: 20.0,
            y: 20.0,
            width: 20.0,
            height: 10.0,
        };
        scroll_owner.children.push(label);
        root.children.push(scroll_owner);

        let hit = pointer_event_handler_hit(&root, 24.0, 24.0, "scroll").expect("scroll hit");
        let old = find_node_path_at(&root, 24.0, 24.0)
            .and_then(|path| {
                path.into_iter()
                    .rev()
                    .find(|key| find_event_handler(&root, key, "scroll").is_some())
            })
            .expect("old scroll target");

        assert_eq!(hit.key, old);
        assert_eq!(hit.node.tag, "box");
        assert_eq!(
            hit.bounds,
            find_node_bounds_by_key(&root, "scroll-owner", 0.0, 0.0).unwrap()
        );
    }

    #[test]
    fn tooltip_container_bounds_finds_innermost_clipping_ancestor() {
        use mesh_core_elements::style::Overflow;

        let mut root = indexed_tree(6, 8);
        assert_eq!(find_tooltip_container_bounds(&root, "cell-2-3"), None);
        assert_eq!(find_tooltip_container_bounds(&root, "missing"), None);

        root.computed_style.overflow_y = Overflow::Hidden;
        assert_eq!(
            find_tooltip_container_bounds(&root, "cell-2-3"),
            Some((0.0, 0.0, 160.0, 120.0))
        );

        root.children[2].computed_style.overflow_y = Overflow::Scroll;
        assert_eq!(
            find_tooltip_container_bounds(&root, "cell-2-3"),
            Some(find_node_bounds_by_key(&root, "row-2", 0.0, 0.0).unwrap())
        );

        assert_eq!(
            find_tooltip_container_bounds(&root, "row-2"),
            Some((0.0, 0.0, 160.0, 120.0))
        );
    }

    #[test]
    fn tooltip_target_lookup_matches_separate_owner_and_bounds_walks() {
        let mut root = indexed_tree(6, 8);
        root.children[4]
            .attributes
            .insert("tooltip".into(), "Inherited row tooltip".into());
        root.children[4].computed_style.transform.translate_x = 7.0;
        root.children[4]
            .attributes
            .insert("_mesh_scroll_x".into(), "3".into());

        let (owner_key, text) = find_tooltip_by_key(&root, "cell-4-3").unwrap();
        let expected_owner = find_node_by_key(&root, &owner_key).unwrap();
        let expected_bounds = find_node_bounds_by_key(&root, &owner_key, 0.0, 0.0).unwrap();

        let target = find_tooltip_target_by_key(&root, "cell-4-3").unwrap();
        assert!(std::ptr::eq(target.owner, expected_owner));
        assert_eq!(target.text, text);
        assert_eq!(target.bounds, expected_bounds);
        assert!(find_tooltip_target_by_key(&root, "missing").is_none());
    }

    #[test]
    fn tooltip_target_cache_reuses_generation_and_refreshes_on_change() {
        let mut root = indexed_tree(4, 4);
        root.children[2]
            .attributes
            .insert("tooltip".into(), "Initial tooltip".into());
        let mut cache = TooltipTargetCache::default();

        let initial = cache.resolve(&root, "cell-2-3", 7, None).unwrap().clone();
        assert_eq!(initial.text.as_ref(), "Initial tooltip");
        let initial_text = Arc::clone(&initial.text);

        root.children[2]
            .attributes
            .insert("tooltip".into(), "Updated tooltip".into());
        assert_eq!(
            cache.resolve(&root, "cell-2-3", 7, None).unwrap(),
            &initial,
            "the same retained generation must reuse the cached snapshot"
        );
        assert!(Arc::ptr_eq(
            &cache.resolve(&root, "cell-2-3", 7, None).unwrap().text,
            &initial_text
        ));
        let refreshed = cache.resolve(&root, "cell-2-3", 8, None).unwrap();
        assert_eq!(refreshed.text.as_ref(), "Updated tooltip");
        assert!(
            !Arc::ptr_eq(&refreshed.text, &initial_text),
            "a new retained generation must refresh tree-derived fields"
        );

        cache.clear();
        assert_eq!(
            cache
                .resolve(&root, "cell-2-3", 8, None)
                .unwrap()
                .text
                .as_ref(),
            "Updated tooltip"
        );
        assert!(cache.resolve(&root, "missing", 8, None).is_none());
    }

    #[test]
    fn tooltip_target_cache_preserves_anonymous_owner_fallback() {
        let fallback = Some((5.0, 6.0, 15.0, 16.0));
        let mut root = WidgetNode::new("box");
        root.attributes.insert("tooltip".into(), "Anonymous".into());
        let mut child = WidgetNode::new("text");
        child.attributes.insert("_mesh_key".into(), "child".into());
        root.children.push(child);

        let mut cache = TooltipTargetCache::default();
        let target = cache.resolve(&root, "child", 1, fallback).unwrap();
        assert_eq!(target.bounds, fallback);
        assert_eq!(target.anchor, TooltipAnchor::Auto);
        assert_eq!(target.offset, None);
    }

    #[test]
    fn find_nodes_by_keys_matches_separate_lookups() {
        let root = indexed_tree(6, 8);
        let keys: std::collections::HashSet<&str> =
            ["row-2", "cell-4-3", "missing-key"].into_iter().collect();

        let found = find_nodes_by_keys(&root, &keys);

        assert_eq!(found.len(), 2, "the missing key must not appear");
        let (row_node, row_bounds) = found.get("row-2").expect("row-2 found");
        assert_eq!(row_node.mesh_key().unwrap(), "row-2");
        assert_eq!(
            row_bounds,
            &find_node_bounds_by_key(&root, "row-2", 0.0, 0.0).unwrap()
        );

        let (cell_node, cell_bounds) = found.get("cell-4-3").expect("cell-4-3 found");
        assert_eq!(cell_node.mesh_key().unwrap(), "cell-4-3");
        assert_eq!(
            cell_bounds,
            &find_node_bounds_by_key(&root, "cell-4-3", 0.0, 0.0).unwrap()
        );
    }

    #[test]
    fn find_nodes_by_keys_respects_transform_offsets() {
        let mut root = indexed_tree(3, 3);
        root.children[1].computed_style.transform.translate_x = 7.0;
        let keys: std::collections::HashSet<&str> = ["cell-1-1"].into_iter().collect();

        let found = find_nodes_by_keys(&root, &keys);

        let (_, bounds) = found.get("cell-1-1").expect("cell-1-1 found");
        assert_eq!(
            bounds,
            &find_node_bounds_by_key(&root, "cell-1-1", 0.0, 0.0).unwrap()
        );
    }

    #[test]
    fn transformed_pointer_bounds_match_the_shared_paint_geometry() {
        let mut root = WidgetNode::new("surface");
        root.layout = LayoutRect {
            x: 0.0,
            y: 0.0,
            width: 200.0,
            height: 100.0,
        };
        let mut button = WidgetNode::new("button");
        button
            .attributes
            .insert("_mesh_key".into(), "button".into());
        button.layout = LayoutRect {
            x: 50.0,
            y: 20.0,
            width: 20.0,
            height: 20.0,
        };
        button.computed_style.transform.scale_x = 2.0;
        button.computed_style.transform.scale_y = 1.5;
        root.children.push(button);

        let bounds = find_node_bounds_by_key(&root, "button", 0.0, 0.0).expect("button bounds");
        assert_eq!(bounds, (40.0, 15.0, 80.0, 45.0));
        let hit = pointer_press_hit(&root, 75.0, 30.0);
        assert_eq!(hit.target.map(|target| target.key), Some("button"));
        assert_eq!(hit.target.map(|target| target.bounds), Some(bounds));
        assert_eq!(
            find_focusable_at(&root, 75.0, 30.0).as_deref(),
            Some("button")
        );
    }

    #[test]
    fn nested_rotation_uses_one_transform_for_hit_focus_and_clip_geometry() {
        let mut root = WidgetNode::new("surface");
        root.layout = LayoutRect {
            x: 0.0,
            y: 0.0,
            width: 240.0,
            height: 200.0,
        };
        let mut parent = WidgetNode::new("box");
        parent.layout = LayoutRect {
            x: 50.0,
            y: 50.0,
            width: 100.0,
            height: 100.0,
        };
        parent.computed_style.transform.rotation = std::f32::consts::FRAC_PI_2;
        parent.computed_style.overflow_x = mesh_core_elements::style::Overflow::Hidden;
        parent.computed_style.overflow_y = mesh_core_elements::style::Overflow::Hidden;

        let mut child = WidgetNode::new("button");
        child.attributes.insert("_mesh_key".into(), "child".into());
        child.layout = LayoutRect {
            x: 80.0,
            y: 80.0,
            width: 20.0,
            height: 20.0,
        };
        let parent_world = node_world_transform(root_transform(0.0, 0.0), &parent);
        let child_parent = child_world_transform(parent_world, &parent);
        let child_world = node_world_transform(child_parent, &child);
        let point = child_world.transform_point(10.0, 10.0);
        parent.children.push(child);
        root.children.push(parent);

        let hit = pointer_press_hit(&root, point.0, point.1);
        assert_eq!(hit.target.map(|target| target.key), Some("child"));
        assert_eq!(
            find_focusable_at(&root, point.0, point.1).as_deref(),
            Some("child")
        );
        assert_eq!(
            find_node_path_at(&root, point.0, point.1),
            Some(vec!["child".into()])
        );
    }

    #[test]
    fn disabled_and_inert_targets_are_filtered_across_interaction_queries() {
        let mut root = WidgetNode::new("surface");
        root.layout = LayoutRect {
            x: 0.0,
            y: 0.0,
            width: 200.0,
            height: 100.0,
        };

        let mut disabled = WidgetNode::new("button");
        disabled
            .attributes
            .insert("_mesh_key".into(), "disabled".into());
        disabled
            .attributes
            .insert("aria-disabled".into(), "true".into());
        disabled
            .attributes
            .insert("tooltip".into(), "Disabled".into());
        disabled
            .event_handlers
            .insert("click".into(), "activate".into());
        disabled.layout = LayoutRect {
            x: 10.0,
            y: 10.0,
            width: 40.0,
            height: 30.0,
        };
        root.children.push(disabled);

        let mut inert = WidgetNode::new("box");
        inert.attributes.insert("_mesh_key".into(), "inert".into());
        inert.attributes.insert("inert".into(), "true".into());
        inert.layout = LayoutRect {
            x: 100.0,
            y: 10.0,
            width: 80.0,
            height: 60.0,
        };
        inert.computed_style.overflow_y = mesh_core_elements::style::Overflow::Scroll;
        inert.scroll_metrics = Some(mesh_core_elements::WidgetScrollMetrics {
            max_y: 20.0,
            content_height: 80.0,
            ..Default::default()
        });
        let mut descendant = WidgetNode::new("button");
        descendant
            .attributes
            .insert("_mesh_key".into(), "inert-descendant".into());
        descendant
            .event_handlers
            .insert("click".into(), "activate".into());
        descendant.layout = LayoutRect {
            x: 5.0,
            y: 5.0,
            width: 30.0,
            height: 20.0,
        };
        inert.children.push(descendant);
        root.children.push(inert);

        assert!(pointer_press_hit(&root, 20.0, 20.0).target.is_none());
        assert!(pointer_event_handler_hit(&root, 20.0, 20.0, "click").is_none());
        assert_eq!(find_focusable_at(&root, 20.0, 20.0), None);
        assert!(
            !collect_focus_traversal(&root)
                .iter()
                .any(|key| key == "disabled" || key == "inert-descendant")
        );
        assert_eq!(find_tooltip_text_by_key(&root, "disabled"), None);
        assert_eq!(find_node_bounds_by_key(&root, "disabled", 0.0, 0.0), None);
        assert_eq!(find_scrollable_at(&root, 110.0, 20.0), None);
        assert!(pointer_press_hit(&root, 110.0, 20.0).target.is_none());
    }

    // cargo test -p mesh-core-interaction --release -- borrowed_find_nodes_keys_beat_owned_results --ignored --nocapture
    #[test]
    #[ignore = "release-only borrowed node-key result microbenchmark"]
    fn borrowed_find_nodes_keys_beat_owned_results() {
        use std::hint::black_box;
        use std::time::Instant;

        fn collect_owned<'a>(
            node: &'a WidgetNode,
            keys: &std::collections::HashSet<&str>,
            offset_x: f32,
            offset_y: f32,
            found: &mut std::collections::HashMap<String, (&'a WidgetNode, ContentBounds)>,
        ) {
            if found.len() == keys.len() {
                return;
            }
            let (offset_x, offset_y) = apply_transform_offset(node, offset_x, offset_y);
            if let Some(key) = node.mesh_key()
                && keys.contains(key)
            {
                found.insert(
                    key.to_owned(),
                    (node, node_rect_with_offset(node, offset_x, offset_y)),
                );
            }
            let (child_offset_x, child_offset_y) =
                child_offsets_with_scroll(node, offset_x, offset_y);
            for child in &node.children {
                collect_owned(child, keys, child_offset_x, child_offset_y, found);
                if found.len() == keys.len() {
                    break;
                }
            }
        }

        let root = indexed_tree(32, 8);
        let owned_keys: Vec<String> = root
            .children
            .iter()
            .flat_map(|row| row.children.iter())
            .filter_map(|node| node.mesh_key().map(str::to_owned))
            .collect();
        let keys: std::collections::HashSet<&str> = owned_keys.iter().map(String::as_str).collect();
        let iterations = 20_000usize;

        let owned_started = Instant::now();
        let mut owned_total = 0usize;
        for _ in 0..iterations {
            let mut found = std::collections::HashMap::with_capacity(keys.len());
            collect_owned(black_box(&root), black_box(&keys), 0.0, 0.0, &mut found);
            owned_total = owned_total.wrapping_add(found.len());
            black_box(found);
        }
        let owned = owned_started.elapsed();

        let borrowed_started = Instant::now();
        let mut borrowed_total = 0usize;
        for _ in 0..iterations {
            let found = find_nodes_by_keys(black_box(&root), black_box(&keys));
            borrowed_total = borrowed_total.wrapping_add(found.len());
            black_box(found);
        }
        let borrowed = borrowed_started.elapsed();

        let speedup = owned.as_secs_f64() / borrowed.as_secs_f64();
        eprintln!(
            "MESH_PERF metric=borrowed_find_nodes_keys_speedup value={speedup:.3} owned={owned:?} borrowed={borrowed:?}"
        );
        assert_eq!(owned_total, borrowed_total);
        assert!(borrowed < owned);
    }

    // cargo test -p mesh-core-interaction --release -- find_nodes_by_keys_beats_per_key_lookups --ignored --nocapture
    #[test]
    #[ignore = "release-only per-key-lookup microbenchmark"]
    fn find_nodes_by_keys_beats_per_key_lookups() {
        use std::hint::black_box;
        use std::time::Instant;

        let root = indexed_tree(200, 20);
        let keys = ["row-198", "cell-199-19", "cell-0-0", "row-100"];
        let key_set: std::collections::HashSet<&str> = keys.into_iter().collect();
        let iterations = 20_000;

        let per_key_started = Instant::now();
        for _ in 0..iterations {
            for key in keys {
                black_box(find_node_by_key(black_box(&root), key));
                black_box(find_node_bounds_by_key(black_box(&root), key, 0.0, 0.0));
            }
        }
        let per_key = per_key_started.elapsed();

        let fused_started = Instant::now();
        for _ in 0..iterations {
            black_box(find_nodes_by_keys(black_box(&root), &key_set));
        }
        let fused = fused_started.elapsed();

        eprintln!(
            "per-key find_node_by_key+bounds: {per_key:?}; fused find_nodes_by_keys: {fused:?}; ratio: {:.1}x",
            per_key.as_secs_f64() / fused.as_secs_f64()
        );
        assert!(fused < per_key);
    }

    #[test]
    #[ignore = "release-only pointer-motion microbenchmark"]
    fn fused_pointer_motion_beats_repeated_tree_walks() {
        use std::hint::black_box;
        use std::time::Instant;

        let tree = indexed_tree(100, 10);
        let iterations = 20_000;
        let started = Instant::now();
        for i in 0..iterations {
            let x = ((i * 17) % 200) as f32;
            let y = ((i * 31) % 2_000) as f32;
            let path = find_node_path_at(&tree, x, y).unwrap_or_default();
            let key = path.last().map(String::as_str).unwrap_or_default();
            black_box(find_tooltip_by_key(&tree, key));
            black_box(find_tooltip_by_key(&tree, key));
            black_box(find_node_bounds_by_key(&tree, key, 0.0, 0.0));
        }
        let tree_walk = started.elapsed();

        let started = Instant::now();
        for i in 0..iterations {
            let x = ((i * 17) % 200) as f32;
            let y = ((i * 31) % 2_000) as f32;
            black_box(pointer_hit_test(&tree, x, y));
        }
        let fused = started.elapsed();

        eprintln!("tree_walk={tree_walk:?} fused={fused:?}");
        assert!(fused < tree_walk, "fused lookup must improve pointer time");
    }

    // cargo test -p mesh-core-interaction --release -- borrowed_tooltip_traversal_beats_eager_string_allocation --ignored --nocapture
    #[test]
    #[ignore = "release-only tooltip traversal allocation microbenchmark"]
    fn borrowed_tooltip_traversal_beats_eager_string_allocation() {
        use std::hint::black_box;
        use std::time::Instant;

        fn legacy_find(
            node: &WidgetNode,
            key: &str,
            inherited: Option<&(String, String)>,
        ) -> Option<Option<(String, String)>> {
            let owner_text = node_tooltip_text(node).map(|text| {
                let owner = node
                    .mesh_key()
                    .map(str::to_owned)
                    .unwrap_or_else(|| format!("anonymous-tooltip-owner:{node:p}"));
                (owner, text)
            });
            let inherited = owner_text.as_ref().or(inherited);
            if node.mesh_key().is_some_and(|candidate| candidate == key) {
                return Some(inherited.cloned());
            }
            for child in &node.children {
                if let Some(found) = legacy_find(child, key, inherited) {
                    return Some(found);
                }
            }
            None
        }

        let mut tree = WidgetNode::new("box");
        tree.attributes.insert("_mesh_key".into(), "node-0".into());
        tree.attributes.insert("tooltip".into(), "tooltip-0".into());
        let mut cursor = &mut tree;
        for depth in 1..64 {
            let mut child = WidgetNode::new("box");
            child
                .attributes
                .insert("_mesh_key".into(), format!("node-{depth}"));
            child
                .attributes
                .insert("tooltip".into(), format!("tooltip-{depth}"));
            cursor.children.push(child);
            cursor = cursor.children.last_mut().unwrap();
        }
        let iterations = 100_000;

        let eager_started = Instant::now();
        let mut eager_bytes = 0usize;
        for _ in 0..iterations {
            let (owner, text) = legacy_find(black_box(&tree), black_box("node-63"), None)
                .flatten()
                .unwrap();
            eager_bytes = eager_bytes.wrapping_add(owner.len() + text.len());
        }
        let eager = eager_started.elapsed();

        let borrowed_started = Instant::now();
        let mut borrowed_bytes = 0usize;
        for _ in 0..iterations {
            let (owner, text) =
                find_tooltip_by_key(black_box(&tree), black_box("node-63")).unwrap();
            borrowed_bytes = borrowed_bytes.wrapping_add(owner.len() + text.len());
        }
        let borrowed = borrowed_started.elapsed();

        let speedup = eager.as_secs_f64() / borrowed.as_secs_f64();
        eprintln!(
            "MESH_PERF metric=borrowed_tooltip_traversal_speedup value={speedup:.3} eager={eager:?} borrowed={borrowed:?}"
        );
        assert_eq!(eager_bytes, borrowed_bytes);
        assert!(borrowed < eager);
    }

    // cargo test -p mesh-core-interaction --release -- fused_tooltip_target_lookup_beats_separate_tree_walks --ignored --nocapture
    #[test]
    #[ignore = "release-only fused tooltip target lookup benchmark"]
    fn fused_tooltip_target_lookup_beats_separate_tree_walks() {
        use std::hint::black_box;
        use std::time::Instant;

        let mut tree = indexed_tree(200, 12);
        let owner_key = "row-199";
        let hovered_key = "cell-199-11";
        tree.children[199]
            .attributes
            .insert("tooltip".into(), "Deep inherited tooltip".into());
        tree.children[199].computed_style.transform.translate_x = 3.0;
        let iterations = 20_000usize;

        let separate_started = Instant::now();
        let mut separate_total = 0usize;
        for _ in 0..iterations {
            let (resolved_owner, text) =
                find_tooltip_by_key(black_box(&tree), black_box(hovered_key)).unwrap();
            let owner = find_node_by_key(black_box(&tree), black_box(&resolved_owner)).unwrap();
            let bounds =
                find_node_bounds_by_key(black_box(&tree), black_box(&resolved_owner), 0.0, 0.0)
                    .unwrap();
            separate_total = separate_total
                .wrapping_add(text.len())
                .wrapping_add(owner.mesh_key().unwrap().len())
                .wrapping_add(bounds.0 as usize);
        }
        let separate = separate_started.elapsed();

        let fused_started = Instant::now();
        let mut fused_total = 0usize;
        for _ in 0..iterations {
            let target =
                find_tooltip_target_by_key(black_box(&tree), black_box(hovered_key)).unwrap();
            fused_total = fused_total
                .wrapping_add(target.text.len())
                .wrapping_add(target.owner.mesh_key().unwrap().len())
                .wrapping_add(target.bounds.0 as usize);
        }
        let fused = fused_started.elapsed();

        assert_eq!(target_owner_key(&tree, hovered_key), Some(owner_key));
        assert_eq!(separate_total, fused_total);
        let speedup = separate.as_secs_f64() / fused.as_secs_f64();
        eprintln!(
            "MESH_PERF metric=fused_tooltip_target_lookup_speedup value={speedup:.3} separate={separate:?} fused={fused:?}"
        );
        assert!(fused < separate);

        fn target_owner_key<'a>(tree: &'a WidgetNode, key: &str) -> Option<&'a str> {
            find_tooltip_target_by_key(tree, key)?.owner.mesh_key()
        }
    }

    // cargo test -p mesh-core-interaction --release -- stable_tooltip_target_cache_beats_repeated_tree_lookup --ignored --nocapture
    #[test]
    #[ignore = "release-only stable tooltip target cache benchmark"]
    fn stable_tooltip_target_cache_beats_repeated_tree_lookup() {
        use std::hint::black_box;
        use std::time::Instant;

        let mut tree = indexed_tree(200, 12);
        let hovered_key = "cell-199-11";
        tree.children[199]
            .attributes
            .insert("tooltip".into(), "Deep inherited tooltip".into());
        let iterations = 20_000usize;

        let repeated_started = Instant::now();
        let mut repeated_total = 0usize;
        for _ in 0..iterations {
            let target =
                find_tooltip_target_by_key(black_box(&tree), black_box(hovered_key)).unwrap();
            repeated_total = repeated_total
                .wrapping_add(target.text.len())
                .wrapping_add(target.bounds.0 as usize);
        }
        let repeated = repeated_started.elapsed();

        let cached_started = Instant::now();
        let mut cached_total = 0usize;
        let mut cache = TooltipTargetCache::default();
        for _ in 0..iterations {
            let target = cache
                .resolve(
                    black_box(&tree),
                    black_box(hovered_key),
                    black_box(42),
                    None,
                )
                .unwrap();
            cached_total = cached_total
                .wrapping_add(target.text.len())
                .wrapping_add(target.bounds.unwrap().0 as usize);
        }
        let cached = cached_started.elapsed();

        assert_eq!(repeated_total, cached_total);
        let speedup = repeated.as_secs_f64() / cached.as_secs_f64();
        eprintln!(
            "MESH_PERF metric=stable_tooltip_target_cache_speedup value={speedup:.3} repeated={repeated:?} cached={cached:?}"
        );
        assert!(cached < repeated);
    }

    // cargo test -p mesh-core-interaction --release -- arc_tooltip_text_clone_beats_string_clone --ignored --nocapture
    #[test]
    #[ignore = "release-only tooltip text ownership benchmark"]
    fn arc_tooltip_text_clone_beats_string_clone() {
        use std::hint::black_box;
        use std::time::Instant;

        let owned = "Detailed tooltip text with localized state and shortcut hints. ".repeat(8);
        let shared: Arc<str> = Arc::from(owned.as_str());
        let iterations = 1_000_000usize;

        let string_started = Instant::now();
        let mut string_total = 0usize;
        for _ in 0..iterations {
            let cloned = black_box(owned.clone());
            string_total = string_total.wrapping_add(cloned.len());
        }
        let string_time = string_started.elapsed();

        let arc_started = Instant::now();
        let mut arc_total = 0usize;
        for _ in 0..iterations {
            let cloned = black_box(Arc::clone(&shared));
            arc_total = arc_total.wrapping_add(cloned.len());
        }
        let arc_time = arc_started.elapsed();

        assert_eq!(string_total, arc_total);
        let speedup = string_time.as_secs_f64() / arc_time.as_secs_f64();
        eprintln!(
            "MESH_PERF metric=arc_tooltip_text_clone_speedup value={speedup:.3} string={string_time:?} arc={arc_time:?}"
        );
        assert!(arc_time < string_time);
    }

    // cargo test -p mesh-core-interaction --release -- pointer_press_hit_beats_press_path_tree_walks --ignored --nocapture
    #[test]
    #[ignore = "release-only press-hit microbenchmark"]
    fn pointer_press_hit_beats_press_path_tree_walks() {
        use std::hint::black_box;
        use std::time::Instant;

        let mut tree = indexed_tree(200, 12);
        for row in &mut tree.children {
            row.tag = "box".into();
            for cell in &mut row.children {
                cell.tag = "box".into();
                cell.layout.y = row.layout.y;
                cell.event_handlers
                    .insert("click".into(), "handleClick".into());
            }
        }
        let x = tree.layout.width - 5.0;
        let y = tree.layout.height - 5.0;
        let iterations = 20_000usize;

        let old_started = Instant::now();
        let mut old_total = 0usize;
        for _ in 0..iterations {
            let focusable = crate::focus::find_focusable_at(black_box(&tree), x, y);
            let target = focusable.clone().or_else(|| {
                find_node_path_at(&tree, x, y).and_then(|path| {
                    path.into_iter()
                        .rev()
                        .find(|key| find_event_handler(&tree, key, "click").is_some())
                })
            });
            if let Some(key) = target.as_deref() {
                black_box(find_node_bounds_by_key(&tree, key, 0.0, 0.0));
            }
            old_total = old_total.wrapping_add(target.map_or(0, |key| key.len()));
            old_total = old_total.wrapping_add(focusable.map_or(0, |key| key.len()));
        }
        let old_time = old_started.elapsed();

        let new_started = Instant::now();
        let mut new_total = 0usize;
        for _ in 0..iterations {
            let hit = pointer_press_hit(black_box(&tree), x, y);
            new_total = new_total.wrapping_add(hit.target.map_or(0, |node| node.key.len()));
            new_total = new_total.wrapping_add(hit.focusable.map_or(0, |node| node.key.len()));
            black_box(hit.target.map(|node| node.bounds));
        }
        let new_time = new_started.elapsed();

        eprintln!(
            "press hit lookup: multi-walk {old_time:?}; fused {new_time:?}; ratio {:.1}x",
            old_time.as_secs_f64() / new_time.as_secs_f64()
        );
        assert_eq!(old_total, new_total);
        assert!(new_time < old_time);
    }

    // cargo test -p mesh-core-interaction --release -- pointer_event_handler_hit_beats_path_handler_walks --ignored --nocapture
    #[test]
    #[ignore = "release-only event-handler-hit microbenchmark"]
    fn pointer_event_handler_hit_beats_path_handler_walks() {
        use std::hint::black_box;
        use std::time::Instant;

        let mut tree = indexed_tree(200, 12);
        for row in &mut tree.children {
            for cell in &mut row.children {
                cell.layout.y = row.layout.y;
                cell.event_handlers
                    .insert("scroll".into(), "handleScroll".into());
            }
        }
        let x = tree.layout.width - 5.0;
        let y = tree.layout.height - 5.0;
        let iterations = 20_000usize;

        let old_started = Instant::now();
        let mut old_total = 0usize;
        for _ in 0..iterations {
            let target = find_node_path_at(black_box(&tree), x, y).and_then(|path| {
                path.into_iter()
                    .rev()
                    .find(|key| find_event_handler(&tree, key, "scroll").is_some())
            });
            if let Some(key) = target.as_deref() {
                old_total = old_total.wrapping_add(key.len());
                old_total = old_total
                    .wrapping_add(find_node_by_key(&tree, key).map_or(0, |node| node.tag.len()));
                old_total = old_total.wrapping_add(
                    find_node_bounds_by_key(&tree, key, 0.0, 0.0)
                        .map_or(0, |bounds| usize::from(bounds.2 > bounds.0)),
                );
            }
        }
        let old_time = old_started.elapsed();

        let new_started = Instant::now();
        let mut new_total = 0usize;
        for _ in 0..iterations {
            if let Some(hit) = pointer_event_handler_hit(black_box(&tree), x, y, "scroll") {
                new_total = new_total.wrapping_add(hit.key.len());
                new_total = new_total.wrapping_add(hit.node.tag.len());
                new_total = new_total.wrapping_add(usize::from(hit.bounds.2 > hit.bounds.0));
            }
        }
        let new_time = new_started.elapsed();

        eprintln!(
            "event-handler hit lookup: path/key walks {old_time:?}; fused {new_time:?}; ratio {:.1}x",
            old_time.as_secs_f64() / new_time.as_secs_f64()
        );
        assert_eq!(old_total, new_total);
        assert!(new_time < old_time);
    }

    #[test]
    fn phase87_tooltip_attribute_participates_in_inherited_tooltip_lookup() {
        let mut owner = WidgetNode::new("box");
        owner.attributes.insert("_mesh_key".into(), "owner".into());
        owner
            .attributes
            .insert("tooltip".into(), "Open details".into());

        let mut child = WidgetNode::new("icon");
        child.attributes.insert("_mesh_key".into(), "child".into());
        owner.children.push(child);

        assert_eq!(
            find_tooltip_text_by_key(&owner, "child").as_deref(),
            Some("Open details")
        );
        assert_eq!(
            find_tooltip_by_key(&owner, "child"),
            Some(("owner".into(), "Open details".into()))
        );
    }

    #[test]
    fn tooltip_disabled_attribute_suppresses_title_and_accessible_label() {
        let mut node = WidgetNode::new("button");
        node.attributes.insert("_mesh_key".into(), "button".into());
        node.attributes.insert("title".into(), "Open".into());
        node.attributes.insert("aria-label".into(), "Open".into());
        node.attributes
            .insert("data-tooltip-disabled".into(), "true".into());

        assert_eq!(node_tooltip_text(&node), None);
        assert_eq!(find_tooltip_text_by_key(&node, "button"), None);
    }

    #[test]
    fn aria_label_on_a_button_with_visible_text_children_is_not_a_redundant_tooltip() {
        // e.g. clock-button.mesh: <button aria-label="{clock_aria_label}">
        // wrapping <text>{clock_time}</text><text>{clock_date}</text> — the
        // button already shows its own text on screen, so aria-label (the
        // AT-only accessible name, per docs/spec/09-accessibility.md) must
        // not also surface as a hover tooltip repeating it.
        let mut button = WidgetNode::new("button");
        button
            .attributes
            .insert("_mesh_key".into(), "clock-button".into());
        button
            .attributes
            .insert("aria-label".into(), "10:32 AM, Monday August 24".into());

        let time_text = WidgetNode::new("text");
        button.children.push(time_text);

        assert_eq!(node_tooltip_text(&button), None);
    }

    #[test]
    fn title_attribute_still_wins_even_with_visible_text_children() {
        let mut button = WidgetNode::new("button");
        button
            .attributes
            .insert("_mesh_key".into(), "button".into());
        button
            .attributes
            .insert("title".into(), "Open audio controls".into());

        let label_text = WidgetNode::new("text");
        button.children.push(label_text);

        assert_eq!(
            node_tooltip_text(&button).as_deref(),
            Some("Open audio controls")
        );
    }

    #[test]
    fn container_without_own_tooltip_does_not_fall_back_to_aggregated_children_label() {
        let mut container = WidgetNode::new("row");
        container
            .attributes
            .insert("_mesh_key".into(), "container".into());
        // Simulates the ARIA-style accessible name a wrapper gets once its
        // accessibility snapshot concatenates every descendant's visible
        // text — meaningful for assistive tech, not for a hover tooltip.
        container.accessibility.label = Some("Clock Settings Volume".into());

        let child = WidgetNode::new("button");
        container.children.push(child);

        assert_eq!(node_tooltip_text(&container), None);
    }

    #[test]
    fn leaf_node_still_falls_back_to_accessible_label_for_tooltip() {
        let mut leaf = WidgetNode::new("button");
        leaf.attributes.insert("_mesh_key".into(), "leaf".into());
        leaf.accessibility.label = Some("Mute".into());

        assert_eq!(node_tooltip_text(&leaf).as_deref(), Some("Mute"));
    }

    #[test]
    fn hidden_portal_placeholder_does_not_block_previous_sibling_hit_target() {
        let mut root = WidgetNode::new("stack");
        root.attributes.insert("_mesh_key".into(), "root".into());
        root.layout = LayoutRect {
            x: 0.0,
            y: 0.0,
            width: 120.0,
            height: 80.0,
        };

        let mut button = WidgetNode::new("button");
        button
            .attributes
            .insert("_mesh_key".into(), "button".into());
        button
            .event_handlers
            .insert("click".into(), "onClick".into());
        button.layout = LayoutRect {
            x: 10.0,
            y: 10.0,
            width: 40.0,
            height: 40.0,
        };

        let mut placeholder = WidgetNode::new("box");
        placeholder
            .attributes
            .insert("_mesh_key".into(), "portal".into());
        placeholder
            .attributes
            .insert("hidden".into(), "true".into());
        placeholder.layout = LayoutRect {
            x: 10.0,
            y: 10.0,
            width: 40.0,
            height: 40.0,
        };

        root.children.push(button);
        root.children.push(placeholder);

        assert_eq!(
            find_node_path_at(&root, 30.0, 30.0),
            Some(vec!["root".into(), "button".into()])
        );
    }

    #[test]
    fn inspector_hit_finds_deepest_non_interactive_node_and_its_bounds() {
        let mut root = WidgetNode::new("column");
        root.layout = LayoutRect {
            x: 0.0,
            y: 0.0,
            width: 200.0,
            height: 100.0,
        };
        let mut card = WidgetNode::new("box");
        card.layout = LayoutRect {
            x: 20.0,
            y: 10.0,
            width: 80.0,
            height: 40.0,
        };
        let mut label = WidgetNode::new("text");
        label.layout = LayoutRect {
            x: 30.0,
            y: 18.0,
            width: 50.0,
            height: 16.0,
        };
        card.children.push(label);
        root.children.push(card);

        let hit = inspect_hit_test(&root, 40.0, 24.0).expect("text should be inspectable");
        assert_eq!(hit.node.tag, "text");
        assert_eq!(hit.bounds, (30.0, 18.0, 80.0, 34.0));
    }
}
