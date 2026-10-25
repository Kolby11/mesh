//! Core disclosure behavior for `<popover open-on="hover|click">`
//! ([elements guide](../../../../../../docs/frontend/elements.md#popover-disclosure)).
//!
//! A trigger and its popover usually live in different components of one
//! surface, so the open/close policy cannot belong to either script without a
//! bridge of forwarded events. The controller observes the surface's shared
//! hover path, pointer capture, focus, and keys instead, and proposes every
//! change to the popover's owner: it writes a simple `open={variable}`
//! binding and dispatches `openchange` with `{ open, reason }`. Scripts keep
//! authority over `open`; the controller never holds a second copy of it.

use super::*;
use mesh_core_elements::Prop;
use mesh_core_elements::{HOVER_INTENT_CLOSE_DELAY, PopoverOpenOn};

/// Why the controller proposed an open-state change. Passed to `openchange`
/// handlers as `event.reason`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum DisclosureReason {
    /// The pointer entered the trigger.
    Hover,
    /// Hover intent expired after the pointer left trigger and popup.
    Leave,
    /// A press on the trigger of a click-opened popover.
    Trigger,
    /// Keyboard activation of the trigger.
    Keyboard,
    Escape,
    /// A press outside both trigger and popup.
    Outside,
    /// Keyboard focus moved outside both trigger and popup.
    Focus,
    /// The compositor dismissed the popup surface.
    Dismiss,
}

impl DisclosureReason {
    fn as_str(self) -> &'static str {
        match self {
            Self::Hover => "hover",
            Self::Leave => "leave",
            Self::Trigger => "trigger",
            Self::Keyboard => "keyboard",
            Self::Escape => "escape",
            Self::Outside => "outside",
            Self::Focus => "focus",
            Self::Dismiss => "dismiss",
        }
    }
}

#[derive(Debug, Clone)]
struct Disclosure {
    trigger_key: String,
    open_on: PopoverOpenOn,
    open: bool,
    /// Pointer over the trigger or the popup at the last hover transition.
    pointer_inside: bool,
    close_at: Option<Instant>,
}

/// Popovers of one surface that resolve a trigger, keyed by popover node key.
/// Manual popovers are tracked too: their triggers' tooltips are suppressed
/// while they are open.
#[derive(Debug, Default)]
pub(super) struct DisclosureRegistry {
    entries: HashMap<String, Disclosure>,
    /// Popover whose subtree took the current pointer press. A drag that starts
    /// inside the popup (a slider) holds it open until release.
    captured: Option<String>,
    /// Trigger activation keys already handled on press, so their release
    /// does not toggle again.
    key_presses: HashSet<(String, String)>,
}

impl DisclosureRegistry {
    fn managed(&self) -> impl Iterator<Item = (&String, &Disclosure)> {
        self.entries
            .iter()
            .filter(|(_, disclosure)| disclosure.open_on.is_managed())
    }

    pub(super) fn has_entries(&self) -> bool {
        !self.entries.is_empty()
    }

    pub(super) fn next_deadline(&self) -> Option<Instant> {
        self.entries
            .values()
            .filter_map(|disclosure| disclosure.close_at)
            .min()
    }

    /// Whether `owner_key`'s tooltip is hidden because the popover it triggers
    /// is open and would cover or duplicate it.
    pub(super) fn suppresses_tooltip(&self, owner_key: &str) -> bool {
        self.entries
            .values()
            .any(|disclosure| disclosure.open && disclosure.trigger_key == owner_key)
    }

    pub(super) fn record_key_press(&mut self, trigger_key: &str, key: &str) {
        self.key_presses
            .insert((trigger_key.to_owned(), key.to_owned()));
    }

    pub(super) fn take_key_press(&mut self, trigger_key: &str, key: &str) -> bool {
        self.key_presses
            .remove(&(trigger_key.to_owned(), key.to_owned()))
    }

    fn trigger_disclosure(&self, trigger_key: &str) -> Option<(&String, &Disclosure)> {
        self.managed()
            .find(|(_, disclosure)| disclosure.trigger_key == trigger_key)
    }
}

pub(super) fn popover_is_open(node: &WidgetNode) -> bool {
    node.attributes.get(Prop::Open).is_some_and(|value| {
        let value = value.trim();
        !(value.is_empty()
            || value == "0"
            || value.eq_ignore_ascii_case("false")
            || value.eq_ignore_ascii_case("none"))
    })
}

impl FrontendSurfaceComponent {
    /// Whether `node` is a promoted popover wrapper whose popover is closed
    /// and not playing an entrance or exit. Such a subtree is neither painted
    /// in the parent nor mapped as a child surface, so per-frame passes skip
    /// it; it is built, restyled and laid out like any other subtree.
    pub(super) fn is_dormant_popover_wrapper(&self, node: &WidgetNode) -> bool {
        node.is_promoted_popover()
            && node.children.first().is_some_and(|popover| {
                source_element_tag(popover) == "popover"
                    && !popover_is_open(popover)
                    && popover.mesh_key().is_none_or(|key| {
                        !self.closing_child_keys.contains(key)
                            && !self.entering_child_keys.contains(key)
                    })
            })
    }
}

pub(super) fn popover_anchor_reference(popover: &WidgetNode) -> Option<&str> {
    [
        Prop::AnchorRef,
        Prop::AnchorTarget,
        Prop::AnchorElement,
        Prop::Target,
    ]
    .into_iter()
    .find_map(|name| {
        popover
            .attributes
            .get(name)
            .map(|value| value.trim())
            .filter(|value| !value.is_empty())
    })
}

/// Whether `node` is the element an `anchor-ref` names: its key, `ref`, `id`,
/// or `bind:this` binding.
pub(super) fn node_matches_reference(node: &WidgetNode, reference: &str) -> bool {
    node.mesh_key().is_some_and(|key| key == reference)
        || node.bind_this() == Some(reference)
        || [Prop::Ref, Prop::Id].into_iter().any(|name| {
            node.attributes
                .get(name)
                .is_some_and(|value| value == reference)
        })
}

fn find_key_by_reference<'a>(node: &'a WidgetNode, reference: &str) -> Option<&'a str> {
    if node_matches_reference(node, reference) {
        return node.mesh_key();
    }
    node.children
        .iter()
        .find_map(|child| find_key_by_reference(child, reference))
}

fn subtree_contains_key(node: &WidgetNode, key: &str) -> bool {
    node.mesh_key() == Some(key)
        || node
            .children
            .iter()
            .any(|child| subtree_contains_key(child, key))
}

fn collect_popovers<'a>(node: &'a WidgetNode, out: &mut Vec<&'a WidgetNode>) {
    mesh_core_elements::frame_work::visit(mesh_core_elements::frame_work::Pass::DisclosureRefresh);
    if source_element_tag(node) == "popover" && node.mesh_key().is_some() {
        out.push(node);
    }
    for child in &node.children {
        collect_popovers(child, out);
    }
}

impl FrontendSurfaceComponent {
    /// Re-read popovers, their triggers, and their open state from a freshly
    /// built or patched tree. Pointer and deadline state carries over.
    pub(super) fn refresh_disclosures(&mut self, tree: &WidgetNode) {
        let mut popovers = Vec::new();
        collect_popovers(tree, &mut popovers);
        let mut previous = std::mem::take(&mut self.disclosures.entries);
        let mut newly_open_triggers = Vec::new();
        for popover in popovers {
            let Some(trigger_key) = popover_anchor_reference(popover)
                .and_then(|reference| find_key_by_reference(tree, reference))
            else {
                continue;
            };
            let key = popover.mesh_key().expect("collected popovers have keys");
            let open = popover_is_open(popover);
            let carried = previous.remove(key);
            if open && !carried.as_ref().is_some_and(|disclosure| disclosure.open) {
                newly_open_triggers.push(trigger_key.to_string());
            }
            let (pointer_inside, close_at) = carried
                .map(|disclosure| (disclosure.pointer_inside, disclosure.close_at))
                .unwrap_or_default();
            self.disclosures.entries.insert(
                key.to_string(),
                Disclosure {
                    trigger_key: trigger_key.to_string(),
                    open_on: PopoverOpenOn::from_node(popover).unwrap_or_default(),
                    open,
                    pointer_inside,
                    close_at: close_at.filter(|_| open),
                },
            );
        }
        if self
            .disclosures
            .captured
            .as_ref()
            .is_some_and(|key| !self.disclosures.entries.contains_key(key))
        {
            self.disclosures.captured = None;
        }
        self.sync_expanded_triggers();
        for trigger_key in newly_open_triggers {
            self.hide_trigger_tooltip(&trigger_key);
        }
    }

    fn hide_trigger_tooltip(&mut self, trigger_key: &str) {
        if self
            .hovered_tooltip
            .as_ref()
            .is_some_and(|(owner, _)| owner == trigger_key)
        {
            self.hovered_tooltip = None;
            self.hover_start = None;
            self.tooltip_visible = false;
            self.tooltip_appeared_at = None;
            self.tooltip_target_cache.clear();
            self.invalidate_hover_change(true);
        }
    }

    fn sync_expanded_triggers(&mut self) {
        let expanded = self
            .disclosures
            .entries
            .values()
            .filter(|disclosure| disclosure.open)
            .map(|disclosure| runtime_node_id_for_key(&disclosure.trigger_key))
            .collect::<HashSet<_>>();
        if expanded != self.expanded_triggers {
            self.expanded_triggers = expanded;
            self.invalidate_interaction_restyle();
        }
    }

    /// Apply hover intent after the surface hover path changed: entering a
    /// hover trigger opens its popover; leaving trigger and popup schedules a
    /// close that re-entering either cancels.
    pub(super) fn update_disclosure_hover(
        &mut self,
        tree: &WidgetNode,
        previous_path: &[String],
    ) -> Result<Vec<CoreRequest>, ComponentError> {
        let now = Instant::now();
        let mut opens = Vec::new();
        for (key, disclosure) in &mut self.disclosures.entries {
            if disclosure.open_on != PopoverOpenOn::Hover {
                continue;
            }
            let on_trigger = self
                .hovered_event_path
                .iter()
                .any(|entry| *entry == disclosure.trigger_key);
            let inside = on_trigger
                || self.hovered_event_path.iter().any(|entry| entry == key)
                || self.disclosures.captured.as_deref() == Some(key.as_str());
            let entered_trigger = on_trigger
                && !previous_path
                    .iter()
                    .any(|entry| *entry == disclosure.trigger_key);
            disclosure.pointer_inside = inside;
            if inside {
                disclosure.close_at = None;
            } else if disclosure.open && disclosure.close_at.is_none() {
                disclosure.close_at = Some(now + HOVER_INTENT_CLOSE_DELAY);
            }
            if entered_trigger && !disclosure.open {
                opens.push(key.clone());
            }
        }
        let mut requests = Vec::new();
        for key in opens {
            requests.extend(self.set_disclosure_open(tree, &key, true, DisclosureReason::Hover)?);
        }
        Ok(requests)
    }

    /// Handle a primary press at the current hover path: toggle a click
    /// trigger, capture a press inside a popup, and close every open managed
    /// popover the press landed outside of.
    pub(super) fn disclosure_pointer_pressed(
        &mut self,
        tree: &WidgetNode,
    ) -> Result<Vec<CoreRequest>, ComponentError> {
        let mut changes = Vec::new();
        let mut captured = None;
        for (key, disclosure) in self.disclosures.managed() {
            let on_trigger = self
                .hovered_event_path
                .iter()
                .any(|entry| *entry == disclosure.trigger_key);
            let in_popup = self.hovered_event_path.iter().any(|entry| entry == key);
            if in_popup {
                captured = Some(key.clone());
            } else if on_trigger {
                if disclosure.open_on == PopoverOpenOn::Click {
                    changes.push((key.clone(), !disclosure.open, DisclosureReason::Trigger));
                }
            } else if disclosure.open {
                changes.push((key.clone(), false, DisclosureReason::Outside));
            }
        }
        self.disclosures.captured = captured;
        let mut requests = Vec::new();
        for (key, open, reason) in changes {
            requests.extend(self.set_disclosure_open(tree, &key, open, reason)?);
        }
        Ok(requests)
    }

    /// Release a press captured by a popup; hover intent resumes from the
    /// pointer's current position.
    pub(super) fn disclosure_pointer_released(&mut self) {
        let Some(key) = self.disclosures.captured.take() else {
            return;
        };
        let pointer_inside = self.hovered_event_path.iter().any(|entry| *entry == key);
        if let Some(disclosure) = self.disclosures.entries.get_mut(&key) {
            let on_trigger = self
                .hovered_event_path
                .iter()
                .any(|entry| *entry == disclosure.trigger_key);
            disclosure.pointer_inside = pointer_inside || on_trigger;
            if disclosure.open_on == PopoverOpenOn::Hover
                && disclosure.open
                && !disclosure.pointer_inside
            {
                disclosure.close_at = Some(Instant::now() + HOVER_INTENT_CLOSE_DELAY);
            }
        }
    }

    /// Keyboard activation of a managed trigger. A trigger with its own
    /// `onclick` keeps Enter/Space for that action; it opens with ArrowDown.
    /// Returns `None` when `node_key` is not a trigger this key operates.
    pub(super) fn disclosure_trigger_key(
        &mut self,
        tree: &WidgetNode,
        node_key: &str,
        key: &str,
        activation: bool,
    ) -> Result<Option<Vec<CoreRequest>>, ComponentError> {
        let Some((popover_key, open)) = self
            .disclosures
            .trigger_disclosure(node_key)
            .map(|(popover_key, disclosure)| (popover_key.clone(), disclosure.open))
        else {
            return Ok(None);
        };
        let next = if key == "ArrowDown" {
            if open {
                return Ok(None);
            }
            true
        } else if activation {
            let has_click = find_node_by_key(tree, node_key).is_some_and(|node| {
                node.event_handlers.contains_key("click")
                    || node.event_handler_calls.contains_key("click")
            });
            if has_click {
                return Ok(None);
            }
            !open
        } else {
            return Ok(None);
        };
        self.set_disclosure_open(tree, &popover_key, next, DisclosureReason::Keyboard)
            .map(Some)
    }

    /// Escape with focus on a managed trigger or inside its popup closes the
    /// popover and returns focus to the trigger.
    pub(super) fn disclosure_escape(
        &mut self,
        tree: &WidgetNode,
    ) -> Result<Option<Vec<CoreRequest>>, ComponentError> {
        let Some(focused) = self.focused_key.clone() else {
            return Ok(None);
        };
        let target = self.disclosures.managed().find_map(|(key, disclosure)| {
            if !disclosure.open {
                return None;
            }
            let within = disclosure.trigger_key == focused
                || find_node_by_key(tree, key)
                    .is_some_and(|popover| subtree_contains_key(popover, &focused));
            within.then(|| (key.clone(), disclosure.trigger_key.clone()))
        });
        let Some((popover_key, trigger_key)) = target else {
            return Ok(None);
        };
        self.embedded_popover_return_focus = Some(trigger_key);
        self.pending_embedded_popover_focus_restore = true;
        self.set_disclosure_open(tree, &popover_key, false, DisclosureReason::Escape)
            .map(Some)
    }

    /// After keyboard focus moved, close open managed popovers whose trigger
    /// and popup both lost it, unless the pointer still holds them open.
    pub(super) fn disclosure_focus_moved(
        &mut self,
        tree: &WidgetNode,
    ) -> Result<Vec<CoreRequest>, ComponentError> {
        let Some(focused) = self.focused_key.clone() else {
            return Ok(Vec::new());
        };
        let closes = self
            .disclosures
            .managed()
            .filter(|(key, disclosure)| {
                disclosure.open
                    && !disclosure.pointer_inside
                    && disclosure.trigger_key != focused
                    && find_node_by_key(tree, key)
                        .is_some_and(|popover| !subtree_contains_key(popover, &focused))
            })
            .map(|(key, _)| key.clone())
            .collect::<Vec<_>>();
        let mut requests = Vec::new();
        for key in closes {
            requests.extend(self.set_disclosure_open(
                tree,
                &key,
                false,
                DisclosureReason::Focus,
            )?);
        }
        Ok(requests)
    }

    /// The compositor dismissed the child surface showing `node_key` (a
    /// popover or its promoted wrapper); propose closing its popover so the next trigger
    /// press opens it again instead of toggling a popup that is already gone.
    pub(in crate::shell::component) fn disclosure_child_dismissed(
        &mut self,
        tree: &WidgetNode,
        node_key: &str,
    ) -> Result<Vec<CoreRequest>, ComponentError> {
        let Some(popover_key) = find_node_by_key(tree, node_key)
            .and_then(|node| {
                if source_element_tag(node) == "popover" {
                    Some(node)
                } else {
                    node.children.first()
                }
            })
            .filter(|popover| source_element_tag(popover) == "popover" && popover_is_open(popover))
            .and_then(WidgetNode::mesh_key)
            .map(str::to_owned)
        else {
            return Ok(Vec::new());
        };
        self.set_disclosure_open(tree, &popover_key, false, DisclosureReason::Dismiss)
    }

    /// Close popovers whose hover intent expired.
    pub(super) fn run_due_disclosures(
        &mut self,
        tree: &WidgetNode,
        now: Instant,
    ) -> Result<Vec<CoreRequest>, ComponentError> {
        let due = self
            .disclosures
            .entries
            .iter_mut()
            .filter_map(|(key, disclosure)| {
                let expired = disclosure.close_at.is_some_and(|deadline| deadline <= now);
                expired.then(|| {
                    disclosure.close_at = None;
                    key.clone()
                })
            })
            .collect::<Vec<_>>();
        let mut requests = Vec::new();
        for key in due {
            requests.extend(self.set_disclosure_open(
                tree,
                &key,
                false,
                DisclosureReason::Leave,
            )?);
        }
        Ok(requests)
    }

    /// Propose an open-state change to the popover's owner: write its simple
    /// `open` binding, then dispatch `openchange`.
    pub(super) fn set_disclosure_open(
        &mut self,
        tree: &WidgetNode,
        popover_key: &str,
        open: bool,
        reason: DisclosureReason,
    ) -> Result<Vec<CoreRequest>, ComponentError> {
        let Some(popover) = find_node_by_key(tree, popover_key) else {
            return Ok(Vec::new());
        };
        if let Some(disclosure) = self.disclosures.entries.get_mut(popover_key) {
            disclosure.close_at = None;
            if disclosure.open == open {
                return Ok(Vec::new());
            }
            disclosure.open = open;
            if open {
                let trigger_key = disclosure.trigger_key.clone();
                self.hide_trigger_tooltip(&trigger_key);
            }
        }
        // A popover the user opens takes focus however they opened it; one
        // that hover reveals leaves focus where it is.
        if open {
            match reason {
                DisclosureReason::Trigger => self.pending_embedded_popover_focus = Some(false),
                DisclosureReason::Keyboard => self.pending_embedded_popover_focus = Some(true),
                _ => {}
            }
        }
        self.sync_expanded_triggers();
        let mut requests = Vec::new();
        if let Some(binding) = popover.open_binding() {
            let variable = binding.variable.as_ref();
            let owner = binding.owner.as_deref();
            requests.extend(self.run_script_call(owner, "openchange", |script_ctx| {
                script_ctx
                    .assign_global(variable, &serde_json::json!(open))
                    .map(|()| Vec::new())
            })?);
        }
        let event = serde_json::json!({
            "type": "openchange",
            "open": open,
            "reason": reason.as_str(),
        });
        requests.extend(self.call_resolved_node_handler(popover, "openchange", &[event])?);
        Ok(requests)
    }
}
