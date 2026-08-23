use super::backend::{SurfaceEntry, WaylandRole, apply_config, surface_config_fingerprint};
use super::*;
use std::collections::HashSet;
use std::sync::Arc;

const MAX_REPEAT_EVENTS_PER_POLL: usize = 64;
const SURFACE_FOCUS_GRAB_TIMEOUT: Duration = Duration::from_millis(750);

#[derive(Clone, Copy)]
pub(super) enum GestureKind {
    Swipe,
    Pinch,
    Hold,
}

pub(super) struct KeyboardRepeatState {
    pub(super) surface_id: Arc<str>,
    pub(super) key: String,
    pub(super) mods: KeyMods,
    pub(super) ch: Option<char>,
    pub(super) next_at: Instant,
    pub(super) interval: Duration,
}

impl KeyboardRepeatState {
    fn push_due_events(&mut self, now: Instant, events: &mut Vec<DevWindowEvent>) {
        let mut emitted = 0;
        while self.next_at <= now && emitted < MAX_REPEAT_EVENTS_PER_POLL {
            events.push(DevWindowEvent::Key {
                surface_id: self.surface_id.clone(),
                event: DevWindowKeyEvent::Pressed(self.key.clone(), self.mods.clone()),
            });
            if let Some(ch) = self.ch {
                events.push(DevWindowEvent::Char {
                    surface_id: self.surface_id.clone(),
                    ch,
                });
            }
            self.next_at += self.interval;
            emitted += 1;
        }
    }
}

pub(super) struct State {
    pub(super) registry_state: RegistryState,
    pub(super) output_state: OutputState,
    pub(super) compositor_state: CompositorState,
    pub(super) shm: Shm,
    pub(super) layer_shell: LayerShell,
    pub(super) activation_state: Option<ActivationState>,
    pub(super) focus_grab_manager: Option<HyprlandFocusGrabManagerV1>,
    pub(super) viewporter: Option<WpViewporter>,
    pub(super) fractional_scale_manager: Option<WpFractionalScaleManagerV1>,
    pub(super) blur_manager: Option<OrgKdeKwinBlurManager>,
    pub(super) seat_state: SeatState,
    pub(super) activation_seat: Option<wl_seat::WlSeat>,
    pub(super) focus_grab: Option<HyprlandFocusGrabV1>,
    pub(super) focus_grab_surface_id: Option<Arc<str>>,
    pub(super) focus_grab_requested_at: Option<Instant>,
    pub(super) qh: QueueHandle<State>,
    pub(super) pool: Option<SlotPool>,
    pub(super) surfaces: HashMap<String, SurfaceEntry>,
    pub(super) surface_ids_by_wl_id: HashMap<ObjectId, Arc<str>>,
    pub(super) pointer: Option<ThemedPointer>,
    pub(super) pointer_interactive: bool,
    /// `zwp_pointer_gestures_v1` global, bound when the compositor advertises
    /// it. `None` on compositors without the protocol — gesture events simply
    /// never fire, matching the graceful-degradation pattern used for the
    /// other optional globals (`blur_manager`, `focus_grab_manager`, etc).
    pub(super) pointer_gestures: Option<ZwpPointerGesturesV1>,
    pub(super) gesture_swipe: Option<ZwpPointerGestureSwipeV1>,
    pub(super) gesture_pinch: Option<ZwpPointerGesturePinchV1>,
    pub(super) gesture_hold: Option<ZwpPointerGestureHoldV1>,
    pub(super) touch: Option<wl_touch::WlTouch>,
    /// Surface each active touch id last landed on, so `TouchUp`/`Shape`/
    /// `Orientation` (which carry no surface of their own past `Down`) and the
    /// protocol-wide `Cancel` event can still be attributed to a surface id.
    pub(super) touch_surfaces: HashMap<i32, Arc<str>>,
    /// Surface the in-progress trackpad gesture (swipe/pinch/hold) began on.
    /// `begin` carries the surface; `update`/`end` don't, so this threads it
    /// through. Compositors recognize at most one gesture at a time, so a
    /// single field is sufficient.
    pub(super) gesture_surface: Option<Arc<str>>,
    pub(super) gesture_kind: Option<GestureKind>,
    pub(super) keyboard: Option<wl_keyboard::WlKeyboard>,
    pub(super) pointer_focus: Option<Arc<str>>,
    pub(super) keyboard_focus: Option<Arc<str>>,
    pub(super) keyboard_mods: Modifiers,
    pub(super) keyboard_repeat_info: RepeatInfo,
    pub(super) keyboard_repeat: Option<KeyboardRepeatState>,
    pub(super) events: Vec<DevWindowEvent>,
    /// `xdg_shell` (`xdg_wm_base`) global, bound when available. Required to
    /// create `xdg_positioner`/`xdg_popup` objects for promoted `<popover>`s.
    pub(super) xdg_shell: Option<XdgShell>,
    /// Lifecycle transitions for popups the compositor dismissed (e.g.
    /// outside-click or parent destruction) and surfaces it closed. Entries are
    /// removed from `surfaces` before an event is exposed.
    pub(super) lifecycle_events: Vec<SurfaceLifecycleEvent>,
    /// `surface_id`s of windows whose close button (or compositor close
    /// binding) was activated. Drained by the shell, which decides what closing
    /// means; unlike `dismissed_popups` the surface is *not* removed here,
    /// because xdg-shell's close is a request the client may decline.
    pub(super) close_requests: Vec<String>,
    /// Once set, the Wayland connection cannot recover in this backend. The
    /// first connection failure tears down every retained object and publishes
    /// one `Lost` event per surface; later failures are idempotent.
    pub(super) connection_lost: Option<String>,
}

impl State {
    pub(super) fn keyboard_repeat_state(
        &self,
        surface_id: &str,
        key: &str,
        mods: KeyMods,
        ch: Option<char>,
        now: Instant,
    ) -> Option<KeyboardRepeatState> {
        keyboard_repeat_state_for(self.keyboard_repeat_info, surface_id, key, mods, ch, now)
    }

    pub(super) fn clear_keyboard_repeat_for_key(&mut self, key: &str) {
        if self
            .keyboard_repeat
            .as_ref()
            .is_some_and(|repeat| repeat.key == key)
        {
            self.keyboard_repeat = None;
        }
    }

    pub(super) fn push_due_keyboard_repeats(&mut self) {
        let Some(repeat) = self.keyboard_repeat.as_mut() else {
            return;
        };
        repeat.push_due_events(Instant::now(), &mut self.events);
    }

    pub(super) fn effective_keyboard_mode_for(
        &self,
        surface_id: &str,
        requested: KeyboardMode,
    ) -> KeyboardMode {
        if requested == KeyboardMode::OnDemand
            && self.focus_grab_surface_id.as_deref() == Some(surface_id)
        {
            KeyboardMode::Exclusive
        } else {
            requested
        }
    }

    pub(super) fn reapply_surface_config(&mut self, surface_id: &str) {
        let effective_keyboard_mode = match self.surfaces.get(surface_id) {
            Some(entry) => self.effective_keyboard_mode_for(surface_id, entry.cfg.keyboard_mode),
            None => return,
        };
        let Some(entry) = self.surfaces.get_mut(surface_id) else {
            return;
        };
        if entry.applied_keyboard_mode == effective_keyboard_mode {
            return;
        }

        // Keyboard interactivity is a layer-shell concept; popups never request
        // OnDemand focus, so there is nothing to reapply for the popup role.
        let Some(layer_surface) = entry.role.as_layer() else {
            return;
        };
        let effective_cfg = entry.cfg.with_keyboard_mode(effective_keyboard_mode);
        tracing::debug!(
            "[focus] layer_shell: reapplying keyboard mode for surface_id={surface_id} mode={:?}",
            effective_keyboard_mode
        );
        apply_config(layer_surface, &effective_cfg);
        layer_surface.commit();
        entry.applied_keyboard_mode = effective_keyboard_mode;
        entry.config_fingerprint = surface_config_fingerprint(&entry.cfg, effective_keyboard_mode);
    }

    pub(super) fn request_surface_focus(&mut self, surface_id: &str, event: &PointerEvent) {
        if self.request_surface_focus_grab(surface_id) {
            return;
        }
        self.request_surface_activation(surface_id, event);
    }

    fn request_surface_focus_grab(&mut self, surface_id: &str) -> bool {
        let Some(manager) = self.focus_grab_manager.as_ref() else {
            return false;
        };
        if self.keyboard_focus.as_deref() == Some(surface_id) {
            if self.focus_grab_surface_id.as_deref() == Some(surface_id) {
                tracing::debug!(
                    "[focus] layer_shell: focus already on grabbed surface_id={surface_id}; releasing stale focus grab"
                );
                self.release_surface_focus_grab(surface_id);
            }
            return true;
        }
        let Some(entry) = self.surfaces.get(surface_id) else {
            return false;
        };
        if entry.cfg.keyboard_mode != KeyboardMode::OnDemand {
            return false;
        }

        let grab = self
            .focus_grab
            .get_or_insert_with(|| manager.create_grab(&self.qh, ()));
        let previous_surface_id = self.focus_grab_surface_id.clone();
        if let Some(previous_surface_id) = self.focus_grab_surface_id.as_deref()
            && previous_surface_id != surface_id
            && let Some(previous_entry) = self.surfaces.get(previous_surface_id)
        {
            grab.remove_surface(previous_entry.wl_surface());
        }

        if self.focus_grab_surface_id.as_deref() != Some(surface_id) {
            tracing::debug!("[focus] layer_shell: starting focus grab for surface_id={surface_id}");
            grab.add_surface(entry.wl_surface());
            grab.commit();
            self.focus_grab_surface_id = Some(Arc::from(surface_id));
            self.focus_grab_requested_at = Some(Instant::now());
            if let Some(previous_surface_id) = previous_surface_id.as_deref()
                && previous_surface_id != surface_id
            {
                self.reapply_surface_config(previous_surface_id);
            }
            self.reapply_surface_config(surface_id);
        }

        true
    }

    pub(super) fn release_expired_surface_focus_grab(&mut self) -> bool {
        let Some(surface_id) = self.focus_grab_surface_id.clone() else {
            return false;
        };
        let Some(requested_at) = self.focus_grab_requested_at else {
            tracing::warn!(
                "[focus] layer_shell: focus grab active for surface_id={surface_id} without request timestamp; releasing"
            );
            self.release_surface_focus_grab(&surface_id);
            return true;
        };
        if let Some(keyboard_focus) = self.keyboard_focus.as_deref() {
            if keyboard_focus != surface_id.as_ref() {
                tracing::debug!(
                    "[focus] layer_shell: focus moved off grabbed surface from={keyboard_focus} to={surface_id}; releasing focus grab"
                );
                self.release_surface_focus_grab(&surface_id);
                return true;
            }
            if requested_at.elapsed() < SURFACE_FOCUS_GRAB_TIMEOUT {
                return false;
            }
            tracing::warn!(
                "[focus] layer_shell: focus stayed on grabbed surface_id={surface_id} for too long; releasing focus grab"
            );
            self.release_surface_focus_grab(&surface_id);
            return true;
        }
        if requested_at.elapsed() < SURFACE_FOCUS_GRAB_TIMEOUT {
            return false;
        }

        tracing::warn!(
            "[focus] layer_shell: focus grab timed out for surface_id={surface_id}; releasing"
        );
        self.release_surface_focus_grab(&surface_id);
        true
    }

    fn request_surface_activation(&self, surface_id: &str, event: &PointerEvent) {
        let Some(activation) = self.activation_state.as_ref() else {
            return;
        };
        if self.keyboard_focus.as_deref() == Some(surface_id) {
            return;
        }
        let Some(entry) = self.surfaces.get(surface_id) else {
            return;
        };
        if entry.cfg.keyboard_mode != KeyboardMode::OnDemand {
            return;
        }
        let Some(seat) = self.activation_seat.clone() else {
            tracing::debug!("[focus] layer_shell: skipping activation request without seat");
            return;
        };
        let PointerEventKind::Press { serial, .. } = event.kind else {
            return;
        };

        tracing::debug!(
            "[focus] layer_shell: requesting activation for surface_id={surface_id} serial={serial}"
        );
        activation.request_token(
            &self.qh,
            RequestData {
                app_id: None,
                seat_and_serial: Some((seat, serial)),
                surface: Some(entry.wl_surface().clone()),
            },
        );
    }

    pub(super) fn release_surface_focus_grab(&mut self, surface_id: &str) {
        self.release_surface_focus_grab_inner(surface_id, true);
    }

    pub(super) fn release_surface_focus_grab_for_teardown(&mut self, surface_id: &str) {
        self.release_surface_focus_grab_inner(surface_id, false);
    }

    pub(super) fn release_focus_grab_for_seat_teardown(&mut self) {
        if let Some(surface_id) = self.focus_grab_surface_id.clone() {
            self.release_surface_focus_grab_for_teardown(&surface_id);
        } else if let Some(grab) = self.focus_grab.take() {
            grab.destroy();
        }
        self.focus_grab_surface_id = None;
        self.focus_grab_requested_at = None;
    }

    fn release_surface_focus_grab_inner(&mut self, surface_id: &str, reapply_config: bool) {
        if self.focus_grab_surface_id.as_deref() != Some(surface_id) {
            return;
        }
        let Some(grab) = self.focus_grab.take() else {
            self.focus_grab_surface_id = None;
            self.focus_grab_requested_at = None;
            if reapply_config {
                self.reapply_surface_config(surface_id);
            }
            return;
        };
        if let Some(entry) = self.surfaces.get(surface_id) {
            tracing::debug!(
                "[focus] layer_shell: releasing focus grab for surface_id={surface_id}"
            );
            grab.remove_surface(entry.wl_surface());
        }
        // Destroy is the protocol's hard release path: it removes an active
        // grab even if a compositor does not process an empty whitelist the way
        // we expect. The next focus request creates a fresh grab object.
        grab.destroy();
        self.focus_grab_surface_id = None;
        self.focus_grab_requested_at = None;
        if reapply_config {
            self.reapply_surface_config(surface_id);
        }
    }

    pub(super) fn insert_surface(&mut self, surface_id: String, entry: SurfaceEntry) {
        let wl_id = entry.wl_surface().id();
        if let Some(previous) = self.surfaces.insert(surface_id.clone(), entry) {
            self.surface_ids_by_wl_id
                .remove(&previous.wl_surface().id());
        }
        self.surface_ids_by_wl_id
            .insert(wl_id, Arc::from(surface_id));
    }

    pub(super) fn remove_surface(&mut self, surface_id: &str) -> Option<SurfaceEntry> {
        let entry = self.surfaces.remove(surface_id)?;
        self.surface_ids_by_wl_id.remove(&entry.wl_surface().id());
        Some(entry)
    }

    /// Remove one live surface and release all presentation-owned auxiliary
    /// protocol objects. Callers use this for explicit destruction and for
    /// compositor-originated close/dismiss callbacks, so teardown remains
    /// idempotent regardless of which lifecycle path arrives first.
    pub(super) fn teardown_surface(&mut self, surface_id: &str) -> bool {
        self.teardown_surface_with_focus(surface_id, true)
    }

    pub(super) fn teardown_surface_after_compositor_event(&mut self, surface_id: &str) -> bool {
        self.teardown_surface_with_focus(surface_id, false)
    }

    /// Cancel input owned by a seat capability before dropping its Wayland
    /// object. Capability removal is independent from surface teardown: the
    /// surfaces remain valid, but no follow-up event may be routed from the
    /// capability that just disappeared.
    pub(super) fn cancel_pointer_input(&mut self) {
        cancel_pointer_input_state(
            &mut self.pointer_focus,
            &mut self.gesture_surface,
            &mut self.gesture_kind,
            &mut self.events,
        );
    }

    pub(super) fn cancel_keyboard_input(&mut self) {
        cancel_keyboard_input_state(
            &mut self.keyboard_focus,
            &mut self.keyboard_mods,
            &mut self.keyboard_repeat,
            &mut self.events,
        );
    }

    pub(super) fn cancel_touch_input(&mut self) {
        cancel_touch_input_state(&mut self.touch_surfaces, &mut self.events);
    }

    pub(super) fn cancel_all_input(&mut self) {
        self.cancel_pointer_input();
        self.cancel_keyboard_input();
        self.cancel_touch_input();
    }

    fn teardown_surface_with_focus(&mut self, surface_id: &str, reapply_focus: bool) -> bool {
        if !self.surfaces.contains_key(surface_id) {
            return false;
        }
        self.cancel_surface_input(surface_id);
        if reapply_focus {
            self.release_surface_focus_grab(surface_id);
        } else {
            self.release_surface_focus_grab_for_teardown(surface_id);
        }
        let Some(entry) = self.remove_surface(surface_id) else {
            return false;
        };
        entry.destroy_auxiliary_protocol_objects();
        true
    }

    /// Cancel input transactions that are still owned by one surface before
    /// its Wayland identity is removed. Follow-up protocol callbacks may
    /// arrive after teardown, so clearing the ownership maps and queued stale
    /// events is part of removing the surface, not an optional shell policy.
    fn cancel_surface_input(&mut self, surface_id: &str) {
        cancel_surface_input_state(
            surface_id,
            &mut self.pointer_focus,
            &mut self.keyboard_focus,
            &mut self.keyboard_mods,
            &mut self.keyboard_repeat,
            &mut self.touch_surfaces,
            &mut self.gesture_surface,
            &mut self.gesture_kind,
            &mut self.events,
        );
    }

    /// Tear down a parent and all popup descendants. The returned ids are in
    /// child-first order so compositor-originated callers can publish a
    /// dismissal for each child before the parent close transition.
    pub(super) fn teardown_surface_tree(&mut self, surface_id: &str) -> Vec<String> {
        self.teardown_surface_tree_with_focus(surface_id, true)
    }

    pub(super) fn teardown_surface_tree_after_compositor_event(
        &mut self,
        surface_id: &str,
    ) -> Vec<String> {
        self.teardown_surface_tree_with_focus(surface_id, false)
    }

    fn teardown_surface_tree_with_focus(
        &mut self,
        surface_id: &str,
        reapply_focus: bool,
    ) -> Vec<String> {
        let children = self
            .surfaces
            .iter()
            .filter_map(|(id, entry)| match &entry.role {
                WaylandRole::Popup(role) if role.parent_id == surface_id => Some(id.clone()),
                _ => None,
            })
            .collect::<Vec<_>>();
        let mut removed = Vec::with_capacity(children.len() + 1);
        for child_id in children {
            if self.teardown_surface_with_focus(&child_id, reapply_focus) {
                removed.push(child_id);
            }
        }
        if self.teardown_surface_with_focus(surface_id, reapply_focus) {
            removed.push(surface_id.to_string());
        }
        removed
    }

    pub(super) fn take_surface_lifecycle_events(&mut self) -> Vec<SurfaceLifecycleEvent> {
        std::mem::take(&mut self.lifecycle_events)
    }

    pub(super) fn take_dismissed_popups(&mut self) -> Vec<String> {
        let events = self.take_surface_lifecycle_events();
        let mut dismissed = Vec::new();
        for event in events {
            match event {
                SurfaceLifecycleEvent::Dismissed { surface_id } => dismissed.push(surface_id),
                other => self.lifecycle_events.push(other),
            }
        }
        dismissed
    }

    pub(super) fn connection_lost_error(&self) -> Option<PresentationError> {
        self.connection_lost
            .as_ref()
            .map(|reason| PresentationError::ConnectionLost(reason.clone()))
    }

    /// Tear down all client-owned protocol and input state after the Wayland
    /// connection has failed. No protocol request is attempted here: the
    /// socket is already unusable, so dropping the proxies and clearing the
    /// identity indexes is the only safe cleanup operation.
    pub(super) fn mark_connection_lost(&mut self, reason: String) {
        if self.connection_lost.is_some() {
            return;
        }
        self.connection_lost = Some(reason.clone());

        let pointer_focus = self.pointer_focus.take();
        let touch_surfaces = self
            .touch_surfaces
            .drain()
            .map(|(_, surface_id)| surface_id)
            .collect::<HashSet<_>>();
        let mut touch_surfaces = touch_surfaces.into_iter().collect::<Vec<_>>();
        touch_surfaces.sort();
        self.keyboard_focus = None;
        self.keyboard_repeat = None;
        self.keyboard_mods = Modifiers::default();
        let gesture_surface = self.gesture_surface.take();
        let gesture_kind = self.gesture_kind.take();
        self.focus_grab.take();
        self.focus_grab_surface_id = None;
        self.focus_grab_requested_at = None;
        self.activation_seat = None;
        self.pointer.take();
        self.touch.take();
        self.keyboard.take();
        self.gesture_swipe.take();
        self.gesture_pinch.take();
        self.gesture_hold.take();

        // Discard queued input addressed to objects that no longer exist, but
        // preserve local cancellation signals for active ownership.
        self.events.clear();
        if let Some(surface_id) = pointer_focus {
            self.events
                .push(DevWindowEvent::PointerLeave { surface_id });
        }
        for surface_id in touch_surfaces {
            self.events.push(DevWindowEvent::TouchCancel { surface_id });
        }
        if let Some(surface_id) = gesture_surface {
            match gesture_kind {
                Some(GestureKind::Swipe) => self.events.push(DevWindowEvent::GestureSwipeEnd {
                    surface_id,
                    cancelled: true,
                }),
                Some(GestureKind::Pinch) => self.events.push(DevWindowEvent::GesturePinchEnd {
                    surface_id,
                    cancelled: true,
                }),
                Some(GestureKind::Hold) => self.events.push(DevWindowEvent::GestureHoldEnd {
                    surface_id,
                    cancelled: true,
                }),
                None => {}
            }
        }
        self.close_requests.clear();

        let mut surface_ids = self.surfaces.keys().cloned().collect::<Vec<_>>();
        surface_ids.sort();
        self.surfaces.clear();
        self.surface_ids_by_wl_id.clear();
        self.pool.take();
        for surface_id in surface_ids {
            self.lifecycle_events.push(SurfaceLifecycleEvent::Lost {
                surface_id,
                reason: reason.clone(),
            });
        }
    }

    pub(super) fn surface_id_for_wl_surface(
        &self,
        surface: &wl_surface::WlSurface,
    ) -> Option<Arc<str>> {
        self.surface_ids_by_wl_id.get(&surface.id()).cloned()
    }

    pub(super) fn bind_fractional_scale(
        &self,
        surface: &wl_surface::WlSurface,
        qh: &QueueHandle<Self>,
        surface_id: String,
    ) -> Option<WpFractionalScaleV1> {
        self.fractional_scale_manager
            .as_ref()
            .map(|mgr| mgr.get_fractional_scale(surface, qh, surface_id))
    }
}

fn cancel_surface_input_state(
    surface_id: &str,
    pointer_focus: &mut Option<Arc<str>>,
    keyboard_focus: &mut Option<Arc<str>>,
    keyboard_mods: &mut Modifiers,
    keyboard_repeat: &mut Option<KeyboardRepeatState>,
    touch_surfaces: &mut HashMap<i32, Arc<str>>,
    gesture_surface: &mut Option<Arc<str>>,
    gesture_kind: &mut Option<GestureKind>,
    events: &mut Vec<DevWindowEvent>,
) {
    // Events already queued for this identity were observed before teardown
    // but cannot be routed safely after it. Retain only the cancellation
    // events emitted below.
    events.retain(|event| crate::event_surface_id(event) != surface_id);

    if pointer_focus
        .as_deref()
        .is_some_and(|focused_surface| focused_surface == surface_id)
    {
        let surface_id = pointer_focus
            .take()
            .expect("pointer focus was present after the identity check");
        events.push(DevWindowEvent::PointerLeave { surface_id });
    }

    let keyboard_focus_owned = keyboard_focus
        .as_deref()
        .is_some_and(|focused_surface| focused_surface == surface_id);
    let keyboard_repeat_owned = keyboard_repeat
        .as_ref()
        .is_some_and(|repeat| repeat.surface_id.as_ref() == surface_id);
    if keyboard_focus_owned {
        *keyboard_focus = None;
        *keyboard_mods = Modifiers::default();
    }
    if keyboard_repeat_owned {
        *keyboard_repeat = None;
    }

    if gesture_surface
        .as_deref()
        .is_some_and(|owned_surface| owned_surface == surface_id)
    {
        let gesture_surface = gesture_surface
            .take()
            .expect("gesture surface was present after the identity check");
        push_cancelled_gesture(Some(gesture_surface), gesture_kind.take(), events);
    }

    let touch_owned = touch_surfaces
        .values()
        .any(|owned_surface| owned_surface.as_ref() == surface_id);
    touch_surfaces.retain(|_, owned_surface| owned_surface.as_ref() != surface_id);
    if touch_owned {
        events.push(DevWindowEvent::TouchCancel {
            surface_id: Arc::from(surface_id),
        });
    }
}

fn cancel_pointer_input_state(
    pointer_focus: &mut Option<Arc<str>>,
    gesture_surface: &mut Option<Arc<str>>,
    gesture_kind: &mut Option<GestureKind>,
    events: &mut Vec<DevWindowEvent>,
) {
    events.retain(|event| !is_pointer_event(event));
    if let Some(surface_id) = pointer_focus.take() {
        events.push(DevWindowEvent::PointerLeave { surface_id });
    }
    push_cancelled_gesture(gesture_surface.take(), gesture_kind.take(), events);
}

fn cancel_keyboard_input_state(
    keyboard_focus: &mut Option<Arc<str>>,
    keyboard_mods: &mut Modifiers,
    keyboard_repeat: &mut Option<KeyboardRepeatState>,
    events: &mut Vec<DevWindowEvent>,
) {
    events.retain(|event| !is_keyboard_event(event));
    keyboard_focus.take();
    *keyboard_mods = Modifiers::default();
    keyboard_repeat.take();
}

fn cancel_touch_input_state(
    touch_surfaces: &mut HashMap<i32, Arc<str>>,
    events: &mut Vec<DevWindowEvent>,
) {
    events.retain(|event| !is_touch_event(event));
    let mut surfaces: Vec<Arc<str>> = touch_surfaces.drain().map(|(_, id)| id).collect();
    surfaces.sort();
    surfaces.dedup();
    for surface_id in surfaces {
        events.push(DevWindowEvent::TouchCancel { surface_id });
    }
}

fn push_cancelled_gesture(
    gesture_surface: Option<Arc<str>>,
    gesture_kind: Option<GestureKind>,
    events: &mut Vec<DevWindowEvent>,
) {
    let Some(surface_id) = gesture_surface else {
        return;
    };
    match gesture_kind {
        Some(GestureKind::Swipe) => events.push(DevWindowEvent::GestureSwipeEnd {
            surface_id,
            cancelled: true,
        }),
        Some(GestureKind::Pinch) => events.push(DevWindowEvent::GesturePinchEnd {
            surface_id,
            cancelled: true,
        }),
        Some(GestureKind::Hold) => events.push(DevWindowEvent::GestureHoldEnd {
            surface_id,
            cancelled: true,
        }),
        None => {}
    }
}

fn is_pointer_event(event: &DevWindowEvent) -> bool {
    matches!(
        event,
        DevWindowEvent::PointerMove { .. }
            | DevWindowEvent::PointerLeave { .. }
            | DevWindowEvent::PointerButton { .. }
            | DevWindowEvent::Scroll { .. }
            | DevWindowEvent::TwoFingerScroll { .. }
            | DevWindowEvent::GestureSwipeBegin { .. }
            | DevWindowEvent::GestureSwipeUpdate { .. }
            | DevWindowEvent::GestureSwipeEnd { .. }
            | DevWindowEvent::GesturePinchBegin { .. }
            | DevWindowEvent::GesturePinchUpdate { .. }
            | DevWindowEvent::GesturePinchEnd { .. }
            | DevWindowEvent::GestureHoldBegin { .. }
            | DevWindowEvent::GestureHoldEnd { .. }
    )
}

fn is_keyboard_event(event: &DevWindowEvent) -> bool {
    matches!(
        event,
        DevWindowEvent::Key { .. } | DevWindowEvent::Char { .. }
    )
}

fn is_touch_event(event: &DevWindowEvent) -> bool {
    matches!(
        event,
        DevWindowEvent::TouchDown { .. }
            | DevWindowEvent::TouchMove { .. }
            | DevWindowEvent::TouchUp { .. }
            | DevWindowEvent::TouchCancel { .. }
    )
}

fn keyboard_repeat_state_for(
    repeat_info: RepeatInfo,
    surface_id: &str,
    key: &str,
    mods: KeyMods,
    ch: Option<char>,
    now: Instant,
) -> Option<KeyboardRepeatState> {
    let RepeatInfo::Repeat { rate, delay } = repeat_info else {
        return None;
    };
    if is_non_repeating_key(key) {
        return None;
    }
    let interval = Duration::from_micros((1_000_000 / rate.get() as u64).max(1));
    Some(KeyboardRepeatState {
        surface_id: Arc::from(surface_id),
        key: key.to_string(),
        mods,
        ch,
        next_at: now + Duration::from_millis(delay as u64),
        interval,
    })
}

fn is_non_repeating_key(key: &str) -> bool {
    if key.len() == 1 {
        return false;
    }
    contains_ignore_ascii_case(key, "shift")
        || contains_ignore_ascii_case(key, "control")
        || key.eq_ignore_ascii_case("ctrl")
        || contains_ignore_ascii_case(key, "alt")
        || contains_ignore_ascii_case(key, "super")
        || contains_ignore_ascii_case(key, "meta")
        || key.eq_ignore_ascii_case("capslock")
        || key.eq_ignore_ascii_case("numlock")
        || key.eq_ignore_ascii_case("scrolllock")
        || key.eq_ignore_ascii_case("escape")
        || key.eq_ignore_ascii_case("esc")
}

fn contains_ignore_ascii_case(haystack: &str, needle: &str) -> bool {
    let needle = needle.as_bytes();
    haystack
        .as_bytes()
        .windows(needle.len())
        .any(|window| window.eq_ignore_ascii_case(needle))
}

#[cfg(test)]
mod performance_tests {
    use super::*;
    use std::num::NonZeroU32;
    use std::time::Instant;

    #[test]
    #[ignore = "release-only surface lookup microbenchmark"]
    fn surface_lookup_index_benchmark() {
        let ids: Vec<String> = (0..128).map(|index| format!("surface-{index}")).collect();
        let keys: Vec<u64> = (0..128).collect();
        let indexed: HashMap<u64, String> = keys.iter().copied().zip(ids.iter().cloned()).collect();
        let target_key = *keys.last().unwrap();
        let iterations = 500_000;

        let scan_started = Instant::now();
        let mut scan_len = 0usize;
        for _ in 0..iterations {
            let id = keys
                .iter()
                .zip(ids.iter())
                .find(|(key, _)| **key == target_key)
                .map(|(_, id)| id.clone())
                .unwrap();
            scan_len += id.len();
        }
        let scan = scan_started.elapsed();

        let indexed_started = Instant::now();
        let mut indexed_len = 0usize;
        for _ in 0..iterations {
            let id = indexed.get(&target_key).cloned().unwrap();
            indexed_len += id.len();
        }
        let indexed_elapsed = indexed_started.elapsed();

        assert_eq!(scan_len, indexed_len);
        eprintln!(
            "500k lookups across 128 surfaces: scan {scan:?}; indexed {indexed_elapsed:?}; ratio {:.1}x",
            scan.as_secs_f64() / indexed_elapsed.as_secs_f64()
        );
        assert!(indexed_elapsed < scan);
    }

    #[test]
    fn non_repeating_key_detection_is_case_insensitive() {
        assert!(is_non_repeating_key("Shift_L"));
        assert!(is_non_repeating_key("ISO_Level3_Shift"));
        assert!(is_non_repeating_key("CTRL"));
        assert!(is_non_repeating_key("CapsLock"));
        assert!(is_non_repeating_key("Escape"));
        assert!(is_non_repeating_key("Esc"));
        assert!(!is_non_repeating_key("a"));
        assert!(!is_non_repeating_key("Enter"));
    }

    #[test]
    fn keyboard_repeat_state_skips_non_repeating_keys() {
        let repeat_info = RepeatInfo::Repeat {
            rate: NonZeroU32::new(30).unwrap(),
            delay: 250,
        };
        let mods = KeyMods::default();
        let now = Instant::now();

        assert!(
            keyboard_repeat_state_for(repeat_info, "panel", "Shift_L", mods.clone(), None, now)
                .is_none()
        );
        assert!(
            keyboard_repeat_state_for(repeat_info, "panel", "Esc", mods.clone(), None, now)
                .is_none()
        );

        let repeat =
            keyboard_repeat_state_for(repeat_info, "panel", "a", mods, Some('a'), now).unwrap();
        assert_eq!(repeat.surface_id.as_ref(), "panel");
        assert_eq!(repeat.key, "a");
        assert_eq!(repeat.ch, Some('a'));
        assert_eq!(repeat.next_at, now + Duration::from_millis(250));
    }

    #[test]
    #[ignore = "release-only non-repeating key detection microbenchmark"]
    fn borrowed_non_repeating_key_detection_beats_lowercase_allocation() {
        use std::time::Instant;

        fn old_is_non_repeating_key(key: &str) -> bool {
            let key = key.to_ascii_lowercase();
            key.contains("shift")
                || key.contains("control")
                || key == "ctrl"
                || key.contains("alt")
                || key.contains("super")
                || key.contains("meta")
                || key == "capslock"
                || key == "numlock"
                || key == "scrolllock"
                || key == "escape"
        }

        let keys = [
            "a",
            "Enter",
            "Shift_L",
            "ISO_Level3_Shift",
            "Control_R",
            "Super_L",
            "CapsLock",
            "ArrowLeft",
        ];
        let iterations = 1_000_000;

        let started = Instant::now();
        let mut old_count = 0usize;
        for _ in 0..iterations {
            for key in keys {
                old_count += usize::from(old_is_non_repeating_key(std::hint::black_box(key)));
            }
        }
        let old = started.elapsed();

        let started = Instant::now();
        let mut new_count = 0usize;
        for _ in 0..iterations {
            for key in keys {
                new_count += usize::from(is_non_repeating_key(std::hint::black_box(key)));
            }
        }
        let new = started.elapsed();

        assert_eq!(old_count, new_count);
        eprintln!(
            "non-repeating key detection over {iterations} key batches: lowercase {old:?}, borrowed {new:?}, ratio {:.1}x",
            old.as_secs_f64() / new.as_secs_f64()
        );
        assert!(new < old);
    }

    #[test]
    #[ignore = "release-only key press repeat setup microbenchmark"]
    fn borrowed_repeat_setup_avoids_non_repeating_event_clones() {
        let repeat_info = RepeatInfo::Repeat {
            rate: NonZeroU32::new(30).unwrap(),
            delay: 250,
        };
        let mods = KeyMods {
            ctrl: false,
            shift: true,
            alt: false,
        };
        let iterations = 500_000;
        let now = Instant::now();

        fn old_schedule_keyboard_repeat(
            repeat_info: RepeatInfo,
            surface_id: String,
            key: String,
            mods: KeyMods,
            ch: Option<char>,
            now: Instant,
        ) -> Option<KeyboardRepeatState> {
            keyboard_repeat_state_for(repeat_info, &surface_id, &key, mods, ch, now)
        }

        let started = Instant::now();
        for _ in 0..iterations {
            let surface_id = String::from("@mesh/keyboard/benchmark/surface");
            let name = String::from("Shift_L");
            let key_event = DevWindowEvent::Key {
                surface_id: surface_id.clone().into(),
                event: DevWindowKeyEvent::Pressed(name.clone(), mods.clone()),
            };
            let repeat = old_schedule_keyboard_repeat(
                repeat_info,
                surface_id,
                name,
                mods.clone(),
                None,
                now,
            );
            std::hint::black_box((key_event, repeat));
        }
        let old = started.elapsed();

        let started = Instant::now();
        for _ in 0..iterations {
            let mut surface_id = String::from("@mesh/keyboard/benchmark/surface");
            let name = String::from("Shift_L");
            let repeat =
                keyboard_repeat_state_for(repeat_info, &surface_id, &name, mods.clone(), None, now);
            let key_surface_id = if repeat.is_some() {
                surface_id.clone()
            } else {
                std::mem::take(&mut surface_id)
            };
            let key_event = DevWindowEvent::Key {
                surface_id: key_surface_id.into(),
                event: DevWindowKeyEvent::Pressed(name, mods.clone()),
            };
            std::hint::black_box((key_event, repeat, surface_id));
        }
        let new = started.elapsed();

        eprintln!(
            "non-repeating key press repeat setup over {iterations} events: old {old:?}, borrowed {new:?}, ratio {:.1}x",
            old.as_secs_f64() / new.as_secs_f64()
        );
        assert!(new < old);
    }

    // cargo test -p mesh-core-presentation --release -- repeat_disabled_gate_beats_key_classification --ignored --nocapture
    #[test]
    #[ignore = "release-only disabled repeat setup microbenchmark"]
    fn repeat_disabled_gate_beats_key_classification() {
        fn old_keyboard_repeat_state_for(
            repeat_info: RepeatInfo,
            surface_id: &str,
            key: &str,
            mods: KeyMods,
            ch: Option<char>,
            now: Instant,
        ) -> Option<KeyboardRepeatState> {
            if is_non_repeating_key(key) {
                return None;
            }
            let RepeatInfo::Repeat { rate, delay } = repeat_info else {
                return None;
            };
            let interval = Duration::from_micros((1_000_000 / rate.get() as u64).max(1));
            Some(KeyboardRepeatState {
                surface_id: Arc::from(surface_id),
                key: key.to_string(),
                mods,
                ch,
                next_at: now + Duration::from_millis(delay as u64),
                interval,
            })
        }

        let keys = [
            "Shift_L",
            "ISO_Level3_Shift",
            "Control_R",
            "Super_L",
            "CapsLock",
            "a",
            "Enter",
            "ArrowLeft",
        ];
        let iterations = 500_000usize;
        let now = Instant::now();
        let repeat_info = RepeatInfo::Disable;
        let mods = KeyMods::default();

        let old_started = Instant::now();
        let mut old_count = 0usize;
        for _ in 0..iterations {
            for key in keys {
                old_count += usize::from(
                    old_keyboard_repeat_state_for(
                        repeat_info,
                        "panel",
                        std::hint::black_box(key),
                        mods.clone(),
                        None,
                        now,
                    )
                    .is_some(),
                );
            }
        }
        let old_time = old_started.elapsed();

        let new_started = Instant::now();
        let mut new_count = 0usize;
        for _ in 0..iterations {
            for key in keys {
                new_count += usize::from(
                    keyboard_repeat_state_for(
                        repeat_info,
                        "panel",
                        std::hint::black_box(key),
                        mods.clone(),
                        None,
                        now,
                    )
                    .is_some(),
                );
            }
        }
        let new_time = new_started.elapsed();

        assert_eq!(old_count, new_count);
        eprintln!(
            "disabled repeat setup over {iterations} key batches: classify-first {old_time:?}, repeat-gate-first {new_time:?}, ratio {:.1}x",
            old_time.as_secs_f64() / new_time.as_secs_f64()
        );
        assert!(new_time < old_time);
    }

    // cargo test -p mesh-core-presentation --release -- single_character_repeat_key_skips_modifier_scan --ignored --nocapture
    #[test]
    #[ignore = "release-only single-character repeat-key microbenchmark"]
    fn single_character_repeat_key_skips_modifier_scan() {
        fn old_is_non_repeating_key(key: &str) -> bool {
            contains_ignore_ascii_case(key, "shift")
                || contains_ignore_ascii_case(key, "control")
                || key.eq_ignore_ascii_case("ctrl")
                || contains_ignore_ascii_case(key, "alt")
                || contains_ignore_ascii_case(key, "super")
                || contains_ignore_ascii_case(key, "meta")
                || key.eq_ignore_ascii_case("capslock")
                || key.eq_ignore_ascii_case("numlock")
                || key.eq_ignore_ascii_case("scrolllock")
                || key.eq_ignore_ascii_case("escape")
        }

        let keys = ["a", "b", "1", "=", "Z", ";"];
        let iterations = 1_000_000usize;

        let old_started = Instant::now();
        let mut old_count = 0usize;
        for _ in 0..iterations {
            for key in keys {
                old_count += usize::from(old_is_non_repeating_key(std::hint::black_box(key)));
            }
        }
        let old_time = old_started.elapsed();

        let new_started = Instant::now();
        let mut new_count = 0usize;
        for _ in 0..iterations {
            for key in keys {
                new_count += usize::from(is_non_repeating_key(std::hint::black_box(key)));
            }
        }
        let new_time = new_started.elapsed();

        assert_eq!(old_count, new_count);
        eprintln!(
            "single-character key classification over {iterations} key batches: full scan {old_time:?}, len gate {new_time:?}, ratio {:.1}x",
            old_time.as_secs_f64() / new_time.as_secs_f64()
        );
        assert!(new_time < old_time);
    }

    // cargo test -p mesh-core-presentation --release -- cached_needle_bytes_beats_per_window_as_bytes --ignored --nocapture
    #[test]
    #[ignore = "release-only case-insensitive contains microbenchmark"]
    fn cached_needle_bytes_beats_per_window_as_bytes() {
        fn old_contains_ignore_ascii_case(haystack: &str, needle: &str) -> bool {
            haystack
                .as_bytes()
                .windows(needle.len())
                .any(|window| window.eq_ignore_ascii_case(needle.as_bytes()))
        }

        let keys = [
            "ISO_Level3_Shift",
            "Control_R",
            "Super_L",
            "Pointer_Button_Primary",
            "XF86AudioRaiseVolume",
        ];
        let needles = ["shift", "control", "super", "audio"];
        let iterations = 300_000usize;

        let old_started = Instant::now();
        let mut old_count = 0usize;
        for _ in 0..iterations {
            for key in keys {
                for needle in needles {
                    old_count += usize::from(old_contains_ignore_ascii_case(
                        std::hint::black_box(key),
                        std::hint::black_box(needle),
                    ));
                }
            }
        }
        let old_time = old_started.elapsed();

        let new_started = Instant::now();
        let mut new_count = 0usize;
        for _ in 0..iterations {
            for key in keys {
                for needle in needles {
                    new_count += usize::from(contains_ignore_ascii_case(
                        std::hint::black_box(key),
                        std::hint::black_box(needle),
                    ));
                }
            }
        }
        let new_time = new_started.elapsed();

        assert_eq!(old_count, new_count);
        eprintln!(
            "contains_ignore_ascii_case over {iterations} key batches: per-window needle bytes {old_time:?}, cached needle bytes {new_time:?}, ratio {:.1}x",
            old_time.as_secs_f64() / new_time.as_secs_f64()
        );
        assert!(new_time < old_time);
    }
}

#[cfg(test)]
mod input_teardown_tests {
    use super::*;

    #[test]
    fn surface_input_cancellation_clears_ownership_and_emits_local_cancels() {
        let mut pointer_focus = Some(Arc::from("panel") as Arc<str>);
        let mut keyboard_focus = Some(Arc::from("panel") as Arc<str>);
        let mut keyboard_mods = Modifiers {
            ctrl: true,
            shift: true,
            ..Modifiers::default()
        };
        let mut keyboard_repeat = Some(KeyboardRepeatState {
            surface_id: Arc::from("panel"),
            key: "a".to_string(),
            mods: KeyMods::default(),
            ch: Some('a'),
            next_at: Instant::now(),
            interval: Duration::from_millis(30),
        });
        let mut touch_surfaces = HashMap::from([
            (1, Arc::from("panel") as Arc<str>),
            (2, Arc::from("panel") as Arc<str>),
            (3, Arc::from("other") as Arc<str>),
        ]);
        let mut gesture_surface = Some(Arc::from("panel") as Arc<str>);
        let mut gesture_kind = Some(GestureKind::Pinch);
        let mut events = vec![
            DevWindowEvent::PointerMove {
                surface_id: Arc::from("panel"),
                x: 4.0,
                y: 8.0,
            },
            DevWindowEvent::Key {
                surface_id: Arc::from("panel"),
                event: DevWindowKeyEvent::Pressed("a".to_string(), KeyMods::default()),
            },
            DevWindowEvent::PointerMove {
                surface_id: Arc::from("other"),
                x: 1.0,
                y: 2.0,
            },
        ];

        cancel_surface_input_state(
            "panel",
            &mut pointer_focus,
            &mut keyboard_focus,
            &mut keyboard_mods,
            &mut keyboard_repeat,
            &mut touch_surfaces,
            &mut gesture_surface,
            &mut gesture_kind,
            &mut events,
        );

        assert!(pointer_focus.is_none());
        assert!(keyboard_focus.is_none());
        assert!(keyboard_repeat.is_none());
        assert!(!keyboard_mods.ctrl);
        assert!(!keyboard_mods.shift);
        assert!(gesture_surface.is_none());
        assert!(gesture_kind.is_none());
        assert_eq!(touch_surfaces.len(), 1);
        assert_eq!(touch_surfaces.get(&3).map(Arc::as_ref), Some("other"));

        assert!(events.iter().any(|event| matches!(
            event,
            DevWindowEvent::PointerMove { surface_id, x, y }
                if surface_id.as_ref() == "other" && (*x, *y) == (1.0, 2.0)
        )));
        assert_eq!(
            events
                .iter()
                .filter(|event| crate::event_surface_id(event) == "panel")
                .count(),
            3,
            "only pointer leave, gesture end, and touch cancel should remain"
        );
        assert!(events.iter().any(|event| matches!(
            event,
            DevWindowEvent::PointerLeave { surface_id } if surface_id.as_ref() == "panel"
        )));
        assert!(events.iter().any(|event| matches!(
            event,
            DevWindowEvent::GesturePinchEnd { surface_id, cancelled }
                if surface_id.as_ref() == "panel" && *cancelled
        )));
        assert!(events.iter().any(|event| matches!(
            event,
            DevWindowEvent::TouchCancel { surface_id } if surface_id.as_ref() == "panel"
        )));
    }

    #[test]
    fn surface_input_cancellation_preserves_other_surface_ownership() {
        let mut pointer_focus = Some(Arc::from("other") as Arc<str>);
        let mut keyboard_focus = Some(Arc::from("other") as Arc<str>);
        let mut keyboard_mods = Modifiers::default();
        let mut keyboard_repeat = Some(KeyboardRepeatState {
            surface_id: Arc::from("other"),
            key: "a".to_string(),
            mods: KeyMods::default(),
            ch: Some('a'),
            next_at: Instant::now(),
            interval: Duration::from_millis(30),
        });
        let mut touch_surfaces = HashMap::from([(1, Arc::from("other") as Arc<str>)]);
        let mut gesture_surface = Some(Arc::from("other") as Arc<str>);
        let mut gesture_kind = Some(GestureKind::Swipe);
        let mut events = vec![DevWindowEvent::PointerMove {
            surface_id: Arc::from("other"),
            x: 1.0,
            y: 2.0,
        }];

        cancel_surface_input_state(
            "panel",
            &mut pointer_focus,
            &mut keyboard_focus,
            &mut keyboard_mods,
            &mut keyboard_repeat,
            &mut touch_surfaces,
            &mut gesture_surface,
            &mut gesture_kind,
            &mut events,
        );

        assert_eq!(pointer_focus.as_deref(), Some("other"));
        assert_eq!(keyboard_focus.as_deref(), Some("other"));
        assert!(keyboard_repeat.is_some());
        assert_eq!(touch_surfaces.get(&1).map(Arc::as_ref), Some("other"));
        assert_eq!(gesture_surface.as_deref(), Some("other"));
        assert!(matches!(gesture_kind, Some(GestureKind::Swipe)));
        assert_eq!(events.len(), 1);
        assert!(matches!(
            &events[0],
            DevWindowEvent::PointerMove { surface_id, .. } if surface_id.as_ref() == "other"
        ));
    }
}

#[cfg(test)]
mod input_capability_tests {
    use super::*;

    #[test]
    fn pointer_capability_cancellation_emits_leave_and_gesture_end() {
        let mut pointer_focus = Some(Arc::from("panel") as Arc<str>);
        let mut gesture_surface = Some(Arc::from("panel") as Arc<str>);
        let mut gesture_kind = Some(GestureKind::Pinch);
        let mut events = vec![
            DevWindowEvent::PointerMove {
                surface_id: Arc::from("panel"),
                x: 1.0,
                y: 2.0,
            },
            DevWindowEvent::GesturePinchUpdate {
                surface_id: Arc::from("other"),
                dx: 1.0,
                dy: 2.0,
                scale: 1.0,
                rotation: 0.0,
            },
            DevWindowEvent::Key {
                surface_id: Arc::from("other"),
                event: DevWindowKeyEvent::Pressed("a".to_string(), KeyMods::default()),
            },
        ];

        cancel_pointer_input_state(
            &mut pointer_focus,
            &mut gesture_surface,
            &mut gesture_kind,
            &mut events,
        );

        assert!(pointer_focus.is_none());
        assert!(gesture_surface.is_none());
        assert!(gesture_kind.is_none());
        assert_eq!(
            events
                .iter()
                .filter(|event| is_pointer_event(event))
                .count(),
            2,
            "only the synthetic leave and gesture end should remain"
        );
        assert!(events.iter().any(|event| matches!(
            event,
            DevWindowEvent::PointerLeave { surface_id } if surface_id.as_ref() == "panel"
        )));
        assert!(events.iter().any(|event| matches!(
            event,
            DevWindowEvent::GesturePinchEnd { surface_id, cancelled }
                if surface_id.as_ref() == "panel" && *cancelled
        )));
        assert!(events.iter().any(|event| matches!(
            event,
            DevWindowEvent::Key { surface_id, .. } if surface_id.as_ref() == "other"
        )));
    }

    #[test]
    fn keyboard_capability_cancellation_drops_key_events_and_repeat() {
        let mut keyboard_focus = Some(Arc::from("panel") as Arc<str>);
        let mut keyboard_mods = Modifiers {
            ctrl: true,
            ..Modifiers::default()
        };
        let mut keyboard_repeat = Some(KeyboardRepeatState {
            surface_id: Arc::from("panel"),
            key: "a".to_string(),
            mods: KeyMods::default(),
            ch: Some('a'),
            next_at: Instant::now(),
            interval: Duration::from_millis(30),
        });
        let mut events = vec![
            DevWindowEvent::Key {
                surface_id: Arc::from("panel"),
                event: DevWindowKeyEvent::Pressed("a".to_string(), KeyMods::default()),
            },
            DevWindowEvent::Char {
                surface_id: Arc::from("panel"),
                ch: 'a',
            },
            DevWindowEvent::PointerMove {
                surface_id: Arc::from("other"),
                x: 1.0,
                y: 2.0,
            },
        ];

        cancel_keyboard_input_state(
            &mut keyboard_focus,
            &mut keyboard_mods,
            &mut keyboard_repeat,
            &mut events,
        );

        assert!(keyboard_focus.is_none());
        assert!(keyboard_repeat.is_none());
        assert!(!keyboard_mods.ctrl);
        assert!(!events.iter().any(is_keyboard_event));
        assert!(events.iter().any(|event| matches!(
            event,
            DevWindowEvent::PointerMove { surface_id, .. } if surface_id.as_ref() == "other"
        )));
    }

    #[test]
    fn touch_capability_cancellation_emits_one_cancel_per_surface() {
        let mut touch_surfaces = HashMap::from([
            (1, Arc::from("panel") as Arc<str>),
            (2, Arc::from("panel") as Arc<str>),
            (3, Arc::from("other") as Arc<str>),
        ]);
        let mut events = vec![
            DevWindowEvent::TouchMove {
                surface_id: Arc::from("panel"),
                id: 1,
                x: 1.0,
                y: 2.0,
            },
            DevWindowEvent::TouchUp {
                surface_id: Arc::from("other"),
                id: 3,
            },
            DevWindowEvent::Key {
                surface_id: Arc::from("other"),
                event: DevWindowKeyEvent::Pressed("a".to_string(), KeyMods::default()),
            },
        ];

        cancel_touch_input_state(&mut touch_surfaces, &mut events);

        assert!(touch_surfaces.is_empty());
        assert_eq!(
            events.iter().filter(|event| is_touch_event(event)).count(),
            2,
            "only one cancellation per owned surface should remain"
        );
        assert!(events.iter().any(|event| matches!(
            event,
            DevWindowEvent::Key { surface_id, .. } if surface_id.as_ref() == "other"
        )));
        let cancelled_surfaces: Vec<_> = events
            .iter()
            .filter_map(|event| match event {
                DevWindowEvent::TouchCancel { surface_id } => Some(surface_id.as_ref()),
                _ => None,
            })
            .collect();
        assert_eq!(cancelled_surfaces, ["other", "panel"]);
    }
}

impl ProvidesRegistryState for State {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry_state
    }

    registry_handlers![OutputState, SeatState];
}
