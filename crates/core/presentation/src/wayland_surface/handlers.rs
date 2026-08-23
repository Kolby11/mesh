use super::backend::WaylandRole;
use super::state::{FrameCallbackData, GestureKind};
use super::state::{PendingTextInput, clear_text_input_for_surface};
use super::*;
use std::{borrow::Cow, sync::Arc};

impl Dispatch<wl_callback::WlCallback, FrameCallbackData> for State {
    fn event(
        state: &mut Self,
        _callback: &wl_callback::WlCallback,
        event: wl_callback::Event,
        data: &FrameCallbackData,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        if matches!(event, wl_callback::Event::Done { .. }) {
            state.complete_frame_callback(data);
        }
    }
}

impl CompositorHandler for State {
    fn scale_factor_changed(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        surface: &wl_surface::WlSurface,
        new_factor: i32,
    ) {
        let Some(entry) = self
            .surfaces
            .values_mut()
            .find(|entry| entry.wl_surface() == surface)
        else {
            return;
        };
        // When wp_fractional_scale_v1 is bound for this surface, prefer its
        // more precise preferred_scale events over the deprecated integer path.
        if entry.fractional_scale.is_some() {
            return;
        }
        // Clamp to 1..=3 to prevent extreme scale values from
        // malicious compositors that could cause zero-size or overflow buffers.
        let new_scale = new_factor.clamp(1, 3) as f32;
        if (entry.scale - new_scale).abs() > f32::EPSILON {
            entry.scale = new_scale;
            entry.needs_full_redraw = true;
            tracing::info!(
                scale = new_scale,
                surface_width = entry.width,
                surface_height = entry.height,
                "scale_factor_changed: integer scale update triggered full redraw"
            );
        }
    }

    fn transform_changed(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &wl_surface::WlSurface,
        _new_transform: wl_output::Transform,
    ) {
    }

    fn frame(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        surface: &wl_surface::WlSurface,
        _time: u32,
    ) {
        if let Some(entry) = self
            .surfaces
            .values_mut()
            .find(|entry| entry.wl_surface() == surface)
        {
            // This compatibility path is used only for callbacks created
            // with the legacy surface userdata. MESH's own frame requests use
            // `FrameCallbackData` above, which performs exact generation
            // matching before releasing the pacing gate.
            entry.complete_legacy_frame_callback();
        }
    }

    fn surface_enter(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        surface: &wl_surface::WlSurface,
        output: &wl_output::WlOutput,
    ) {
        // Track which output this surface actually landed on so size
        // resolution (spanning-bar clamp, dynamic-size fallback) can use that
        // output's geometry instead of an arbitrary "first enumerated output"
        // guess. Layer surfaces are created with `output: None` (compositor
        // picks one), so this event is the only way to learn which output it
        // chose — critical on multi-monitor setups where outputs differ in
        // size (a wrong-output guess previously clamped spanning bars to a
        // smaller monitor's width even when placed on a larger one).
        if let Some((surface_id, entry)) = self
            .surfaces
            .iter_mut()
            .find(|(_, entry)| entry.wl_surface() == surface)
        {
            tracing::debug!(surface_id = surface_id.as_str(), "wl_surface::enter fired");
            if entry.enter_output(output.clone()) {
                tracing::debug!(
                    surface_id = surface_id.as_str(),
                    output_generation = entry.surface_generation().output,
                    "wl_surface::enter updated output generation"
                );
            }
        }
    }

    fn surface_leave(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        surface: &wl_surface::WlSurface,
        output: &wl_output::WlOutput,
    ) {
        if let Some((surface_id, entry)) = self
            .surfaces
            .iter_mut()
            .find(|(_, entry)| entry.wl_surface() == surface)
            && entry.leave_output(output)
        {
            tracing::debug!(
                surface_id = surface_id.as_str(),
                output_generation = entry.surface_generation().output,
                "wl_surface::leave cleared output membership"
            );
        }
    }
}

impl OutputHandler for State {
    fn output_state(&mut self) -> &mut OutputState {
        &mut self.output_state
    }

    fn new_output(&mut self, _c: &Connection, _q: &QueueHandle<Self>, _o: wl_output::WlOutput) {}

    fn update_output(
        &mut self,
        _c: &Connection,
        _q: &QueueHandle<Self>,
        output: wl_output::WlOutput,
    ) {
        for (surface_id, entry) in &mut self.surfaces {
            if entry.is_on_output(&output) && entry.mark_output_revision_changed() {
                tracing::debug!(
                    surface_id = surface_id.as_str(),
                    output_generation = entry.surface_generation().output,
                    "wl_output update invalidated surface output revision"
                );
            }
        }
    }

    fn output_destroyed(
        &mut self,
        _c: &Connection,
        _q: &QueueHandle<Self>,
        output: wl_output::WlOutput,
    ) {
        for (surface_id, entry) in &mut self.surfaces {
            if entry.leave_output(&output) {
                tracing::debug!(
                    surface_id = surface_id.as_str(),
                    output_generation = entry.surface_generation().output,
                    "wl_output destruction cleared surface output membership"
                );
            }
        }
    }
}

impl ShmHandler for State {
    fn shm_state(&mut self) -> &mut Shm {
        &mut self.shm
    }
}

impl LayerShellHandler for State {
    fn closed(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, layer: &LayerSurface) {
        let id = self.surface_id_for_wl_surface(layer.wl_surface());
        if let Some(id) = id {
            tracing::debug!(
                "[focus] layer_shell: layer surface closed, releasing focus grab if active for surface_id={id}"
            );
            let removed = self.teardown_surface_tree_after_compositor_event(&id);
            for removed_id in removed {
                if removed_id == id.as_ref() {
                    self.lifecycle_events.push(SurfaceLifecycleEvent::Closed {
                        surface_id: removed_id,
                    });
                } else {
                    self.lifecycle_events
                        .push(SurfaceLifecycleEvent::Dismissed {
                            surface_id: removed_id,
                        });
                }
            }
        }
    }

    fn configure(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        layer: &LayerSurface,
        configure: LayerSurfaceConfigure,
        _serial: u32,
    ) {
        let entry = self
            .surfaces
            .values_mut()
            .find(|entry| entry.wl_surface() == layer.wl_surface());
        if let Some(entry) = entry {
            let (w, h) = configure.new_size;
            if w > 0 {
                entry.width = w;
            }
            if h > 0 {
                entry.height = h;
            }
            entry.accept_configure();
        }
    }
}

impl WindowHandler for State {
    /// The user asked to close the window (title-bar button, compositor
    /// keybind). Per xdg-shell this is a *request*, not a destruction: the
    /// surface stays alive until the client drops it. Record it and let the
    /// shell decide — a module may want to run teardown, or keep the surface
    /// mapped. Dropping the `Window` here would destroy a live component's
    /// compositor object behind its back.
    fn request_close(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, window: &Window) {
        if let Some(id) = self.surface_id_for_wl_surface(window.wl_surface()) {
            tracing::debug!("layer_shell: close requested for window surface_id={id}");
            self.close_requests.push(id.to_string());
        }
    }

    fn configure(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        window: &Window,
        configure: WindowConfigure,
        _serial: u32,
    ) {
        let target = window.wl_surface().clone();
        let Some((surface_id, entry)) = self
            .surfaces
            .iter_mut()
            .find(|(_, entry)| entry.wl_surface() == &target)
        else {
            return;
        };

        // A toplevel configure with no size means "pick your own", which is the
        // usual first configure — leave the CSS-measured size in place. A sized
        // configure is binding: the compositor (tiling layout, maximize,
        // fullscreen, interactive resize) has decided, and content must lay out
        // into it. This is the inverse of the layer-shell direction, where the
        // client's measured size is the request.
        let (width, height) = configure.new_size;
        let size = match (width, height) {
            (Some(width), Some(height)) => Some((width.get(), height.get())),
            _ => None,
        };
        if let Some((width, height)) = size {
            entry.width = width;
            entry.height = height;
        }
        // The states are the compositor's answer to "what is this window now" —
        // they arrive on every configure, including ones with no size. The shell
        // projects them onto the surface tree as CSS state so a module can lay
        // out differently when it fills the output than when it floats.
        let states = WindowStates {
            maximized: configure.is_maximized(),
            fullscreen: configure.is_fullscreen(),
            activated: configure.is_activated(),
            tiled: configure.is_tiled_top()
                || configure.is_tiled_bottom()
                || configure.is_tiled_left()
                || configure.is_tiled_right(),
        };
        if let WaylandRole::Window(role) = &mut entry.role {
            role.compositor_size = size;
            role.states = states;
        }
        entry.accept_configure();
        entry.needs_full_redraw = true;
        tracing::debug!(
            surface_id = surface_id.as_str(),
            ?size,
            ?states,
            "layer_shell: window configure applied"
        );
    }
}

impl SeatHandler for State {
    fn seat_state(&mut self) -> &mut SeatState {
        &mut self.seat_state
    }

    fn new_seat(&mut self, _c: &Connection, _q: &QueueHandle<Self>, seat: wl_seat::WlSeat) {
        let seat_id = seat.id();
        if self.input_seats.contains_key(&seat_id) {
            return;
        }
        self.input_seat_order.push(seat_id.clone());
        self.input_seats
            .insert(seat_id, super::state::SeatInputState::new(seat.clone()));
        self.ensure_text_input_for_seat(&seat);
    }

    fn new_capability(
        &mut self,
        _conn: &Connection,
        qh: &QueueHandle<Self>,
        seat: wl_seat::WlSeat,
        capability: SeatCapability,
    ) {
        let seat_id = seat.id();
        if !self.input_seats.contains_key(&seat_id) {
            self.input_seat_order.push(seat_id.clone());
            self.input_seats.insert(
                seat_id.clone(),
                super::state::SeatInputState::new(seat.clone()),
            );
        }
        self.ensure_text_input_for_seat(&seat);
        self.activation_seat = Some(seat_id.clone());
        if capability == SeatCapability::Pointer
            && self
                .input_seats
                .get(&seat_id)
                .is_some_and(|input| input.pointer.is_none())
        {
            let cursor_surface = self.compositor_state.create_surface(qh);
            if let Ok(ptr) = self.seat_state.get_pointer_with_theme(
                qh,
                &seat,
                self.shm.wl_shm(),
                cursor_surface,
                ThemeSpec::default(),
            ) {
                tracing::debug!("[hover] layer_shell: pointer capability acquired");
                let pointer_id = ptr.pointer().id();
                let mut gesture_handles = None;
                if let Some(gestures) = self.pointer_gestures.as_ref() {
                    let wl_ptr = ptr.pointer();
                    let swipe = gestures.get_swipe_gesture(wl_ptr, qh, GlobalData);
                    let pinch = gestures.get_pinch_gesture(wl_ptr, qh, GlobalData);
                    let hold = gestures.get_hold_gesture(wl_ptr, qh, GlobalData);
                    self.swipe_seats.insert(swipe.id(), seat_id.clone());
                    self.pinch_seats.insert(pinch.id(), seat_id.clone());
                    self.hold_seats.insert(hold.id(), seat_id.clone());
                    gesture_handles = Some((swipe, pinch, hold));
                }
                self.pointer_seats.insert(pointer_id, seat_id.clone());
                if let Some(input) = self.input_seats.get_mut(&seat_id) {
                    input.pointer = Some(ptr);
                    if let Some((swipe, pinch, hold)) = gesture_handles {
                        input.gesture_swipe = Some(swipe);
                        input.gesture_pinch = Some(pinch);
                        input.gesture_hold = Some(hold);
                    }
                }
            }
        }
        if capability == SeatCapability::Touch
            && self
                .input_seats
                .get(&seat_id)
                .is_some_and(|input| input.touch.is_none())
            && let Ok(touch) = self.seat_state.get_touch(qh, &seat)
        {
            self.touch_seats.insert(touch.id(), seat_id.clone());
            if let Some(input) = self.input_seats.get_mut(&seat_id) {
                input.touch = Some(touch);
            }
        }
        if capability == SeatCapability::Keyboard
            && self
                .input_seats
                .get(&seat_id)
                .is_some_and(|input| input.keyboard.is_none())
            && let Ok(kbd) = self.seat_state.get_keyboard(qh, &seat, None)
        {
            self.keyboard_seats.insert(kbd.id(), seat_id.clone());
            if let Some(input) = self.input_seats.get_mut(&seat_id) {
                input.keyboard = Some(kbd);
            }
        }
    }

    fn remove_capability(
        &mut self,
        _c: &Connection,
        _q: &QueueHandle<Self>,
        seat: wl_seat::WlSeat,
        capability: SeatCapability,
    ) {
        let seat_id = seat.id();
        if capability == SeatCapability::Pointer {
            self.cancel_pointer_input_for_seat(&seat_id);
            if let Some(input) = self.input_seats.get_mut(&seat_id) {
                if let Some(gesture) = input.gesture_swipe.take() {
                    self.swipe_seats.remove(&gesture.id());
                    gesture.destroy();
                }
                if let Some(gesture) = input.gesture_pinch.take() {
                    self.pinch_seats.remove(&gesture.id());
                    gesture.destroy();
                }
                if let Some(gesture) = input.gesture_hold.take() {
                    self.hold_seats.remove(&gesture.id());
                    gesture.destroy();
                }
                if let Some(pointer) = input.pointer.take() {
                    self.pointer_seats.remove(&pointer.pointer().id());
                }
            }
        }
        if capability == SeatCapability::Touch {
            self.cancel_touch_input_for_seat(&seat_id);
            if let Some(input) = self.input_seats.get_mut(&seat_id)
                && let Some(touch) = input.touch.take()
            {
                self.touch_seats.remove(&touch.id());
                touch.release();
            }
        }
        if capability == SeatCapability::Keyboard {
            self.cancel_keyboard_input_for_seat(&seat_id);
            self.release_focus_grab_for_seat_teardown(&seat_id);
            if let Some(input) = self.input_seats.get_mut(&seat_id)
                && let Some(keyboard) = input.keyboard.take()
            {
                self.keyboard_seats.remove(&keyboard.id());
                keyboard.release();
            }
        }
    }

    fn remove_seat(&mut self, _c: &Connection, _q: &QueueHandle<Self>, seat: wl_seat::WlSeat) {
        let seat_id = seat.id();
        self.cancel_all_input_for_seat(&seat_id);
        self.release_focus_grab_for_seat_teardown(&seat_id);
        let text_input = self
            .input_seats
            .get_mut(&seat_id)
            .and_then(|input| input.text_input.take());
        if let Some(text_input) = text_input {
            self.text_input_seats.remove(&text_input.id());
            text_input.destroy();
        }
        if let Some(input) = self.input_seats.get_mut(&seat_id) {
            if let Some(gesture) = input.gesture_swipe.take() {
                self.swipe_seats.remove(&gesture.id());
                gesture.destroy();
            }
            if let Some(gesture) = input.gesture_pinch.take() {
                self.pinch_seats.remove(&gesture.id());
                gesture.destroy();
            }
            if let Some(gesture) = input.gesture_hold.take() {
                self.hold_seats.remove(&gesture.id());
                gesture.destroy();
            }
            if let Some(pointer) = input.pointer.take() {
                self.pointer_seats.remove(&pointer.pointer().id());
            }
            if let Some(touch) = input.touch.take() {
                self.touch_seats.remove(&touch.id());
                touch.release();
            }
            if let Some(keyboard) = input.keyboard.take() {
                self.keyboard_seats.remove(&keyboard.id());
                keyboard.release();
            }
        }
        self.input_seats.remove(&seat_id);
        self.input_seat_order.retain(|id| id != &seat_id);
        if self.activation_seat.as_ref() == Some(&seat_id) {
            self.activation_seat = self.input_seat_order.last().cloned();
        }
    }
}

impl PointerHandler for State {
    fn pointer_frame(
        &mut self,
        conn: &Connection,
        _qh: &QueueHandle<Self>,
        pointer: &wl_pointer::WlPointer,
        events: &[PointerEvent],
    ) {
        let Some(seat_id) = self.seat_id_for_pointer(pointer) else {
            return;
        };
        for event in events {
            let surface_id = match self.surface_id_for_wl_surface(&event.surface) {
                Some(id) => id,
                None => continue,
            };
            match event.kind {
                PointerEventKind::Enter { .. } => {
                    tracing::debug!("[hover] layer_shell: pointer enter surface_id={surface_id}");
                    if let Some(input) = self.input_seats.get_mut(&seat_id) {
                        input.pointer_focus = Some(surface_id.clone());
                    }
                    if let Some(pointer) = self
                        .input_seats
                        .get(&seat_id)
                        .and_then(|input| input.pointer.as_ref())
                        && let Err(error) = pointer.set_cursor(
                            conn,
                            if self.pointer_interactive {
                                CursorIcon::Pointer
                            } else {
                                CursorIcon::Default
                            },
                        )
                    {
                        tracing::debug!(
                            "[hover] layer_shell: failed to set cursor on enter: {error}"
                        );
                    }
                    // Emit a synthetic PointerMove at the entry coordinates so the shell
                    // cancels any pending hover-bridge hide immediately on entry rather
                    // than waiting for the first motion event.
                    let (x, y) = (event.position.0 as f32, event.position.1 as f32);
                    self.queue_event(&seat_id, DevWindowEvent::PointerMove { surface_id, x, y });
                }
                PointerEventKind::Leave { .. } => {
                    tracing::debug!("[hover] layer_shell: pointer leave surface_id={surface_id}");
                    let focus_grabbed = self
                        .input_seats
                        .get(&seat_id)
                        .and_then(|input| input.focus_grab_surface_id.as_deref())
                        == Some(surface_id.as_ref());
                    if focus_grabbed {
                        tracing::debug!(
                            "[focus] layer_shell: pointer left grabbed surface_id={surface_id}; releasing focus grab"
                        );
                        self.release_surface_focus_grab_for_seat(&seat_id, &surface_id, true);
                    }
                    if self
                        .input_seats
                        .get(&seat_id)
                        .and_then(|input| input.pointer_focus.as_ref())
                        == Some(&surface_id)
                    {
                        if let Some(input) = self.input_seats.get_mut(&seat_id) {
                            input.pointer_focus = None;
                        }
                    }
                    self.queue_event(&seat_id, DevWindowEvent::PointerLeave { surface_id });
                }
                PointerEventKind::Motion { .. } => {
                    let (x, y) = (event.position.0 as f32, event.position.1 as f32);
                    tracing::trace!(
                        "[hover] layer_shell: pointer motion surface_id={surface_id} x={x:.1} y={y:.1}"
                    );
                    self.queue_event(&seat_id, DevWindowEvent::PointerMove { surface_id, x, y });
                }
                PointerEventKind::Press { button, serial, .. } => {
                    if button == crate::PRIMARY_POINTER_BUTTON {
                        self.request_surface_focus(&seat_id, &surface_id, event);
                    }
                    let (x, y) = (event.position.0 as f32, event.position.1 as f32);
                    tracing::debug!(
                        "[hover] layer_shell: pointer press surface_id={surface_id} button={button} x={x:.1} y={y:.1}"
                    );
                    self.queue_event(
                        &seat_id,
                        DevWindowEvent::PointerButtonWithIdentity {
                            surface_id,
                            x,
                            y,
                            button,
                            pressed: true,
                            identity: crate::PointerButtonIdentity {
                                seat_id: seat_id.protocol_id(),
                                serial,
                            },
                        },
                    );
                }
                PointerEventKind::Release { button, .. } => {
                    let (x, y) = (event.position.0 as f32, event.position.1 as f32);
                    tracing::debug!(
                        "[hover] layer_shell: pointer release surface_id={surface_id} button={button} x={x:.1} y={y:.1}"
                    );
                    self.queue_event(
                        &seat_id,
                        DevWindowEvent::PointerButton {
                            surface_id,
                            x,
                            y,
                            button,
                            pressed: false,
                        },
                    );
                }
                PointerEventKind::Axis {
                    horizontal,
                    vertical,
                    source,
                    ..
                } => {
                    let (x, y) = (event.position.0 as f32, event.position.1 as f32);
                    let dx = normalized_axis_delta(horizontal.absolute, horizontal.discrete);
                    let dy = normalized_axis_delta(vertical.absolute, vertical.discrete);
                    if dx.abs() > f32::EPSILON || dy.abs() > f32::EPSILON {
                        tracing::debug!(
                            surface_id = surface_id.as_ref(),
                            ?source,
                            x,
                            y,
                            dx,
                            dy,
                            "Wayland pointer axis input"
                        );
                        if source == Some(wl_pointer::AxisSource::Finger) {
                            self.queue_event(
                                &seat_id,
                                DevWindowEvent::TwoFingerScroll {
                                    surface_id,
                                    x,
                                    y,
                                    dx,
                                    dy,
                                },
                            );
                        } else {
                            self.queue_event(
                                &seat_id,
                                DevWindowEvent::Scroll {
                                    surface_id,
                                    x,
                                    y,
                                    dx,
                                    dy,
                                },
                            );
                        }
                    }
                }
            }
        }
    }
}

/// Convert SCTK's axis payload into MESH's direction convention (positive is
/// up/left). Compositors normally provide the continuous `absolute` value,
/// but the protocol also permits step-only axis frames. Dropping those frames
/// makes some pointer devices appear completely inert to component handlers.
fn normalized_axis_delta(absolute: f64, discrete: i32) -> f32 {
    let protocol_delta = if absolute.is_finite() && absolute.abs() > f64::EPSILON {
        absolute as f32
    } else {
        discrete as f32
    };
    -protocol_delta
}

impl ActivationHandler for State {
    type RequestData = RequestData;

    fn new_token(&mut self, token: String, data: &Self::RequestData) {
        let Some(activation) = self.activation_state.as_ref() else {
            return;
        };
        let Some(surface) = data.surface.as_ref() else {
            return;
        };
        tracing::debug!("[focus] layer_shell: activating surface via xdg-activation");
        activation.activate::<State>(surface, token);
    }
}

impl Dispatch<HyprlandFocusGrabManagerV1, GlobalData, State> for State {
    fn event(
        _: &mut State,
        _: &HyprlandFocusGrabManagerV1,
        _: hyprland_focus_grab_manager_v1::Event,
        _: &GlobalData,
        _: &Connection,
        _: &QueueHandle<State>,
    ) {
        unreachable!("hyprland_focus_grab_manager_v1 has no events");
    }
}

impl Dispatch<WpViewporter, GlobalData, State> for State {
    fn event(
        _: &mut State,
        _: &WpViewporter,
        _: wp_viewporter::Event,
        _: &GlobalData,
        _: &Connection,
        _: &QueueHandle<State>,
    ) {
        unreachable!("wp_viewporter has no events");
    }
}

impl Dispatch<WpFractionalScaleManagerV1, GlobalData, State> for State {
    fn event(
        _: &mut State,
        _: &WpFractionalScaleManagerV1,
        _: wp_fractional_scale_manager_v1::Event,
        _: &GlobalData,
        _: &Connection,
        _: &QueueHandle<State>,
    ) {
        unreachable!("wp_fractional_scale_manager_v1 has no events");
    }
}

impl Dispatch<HyprlandFocusGrabV1, (), State> for State {
    fn event(
        state: &mut State,
        grab: &HyprlandFocusGrabV1,
        event: hyprland_focus_grab_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<State>,
    ) {
        let Some(seat_id) = state.seat_id_for_focus_grab(grab) else {
            return;
        };
        if let hyprland_focus_grab_v1::Event::Cleared = event {
            tracing::debug!("[focus] layer_shell: compositor cleared focus grab");
            let surface_id = state
                .input_seats
                .get(&seat_id)
                .and_then(|input| input.focus_grab_surface_id.clone());
            if let Some(input) = state.input_seats.get_mut(&seat_id) {
                if let Some(grab) = input.focus_grab.take() {
                    state.focus_grab_seats.remove(&grab.id());
                    grab.destroy();
                }
                input.focus_grab_requested_at = None;
                input.focus_grab_surface_id = None;
            }
            if let Some(surface_id) = surface_id {
                state.reapply_surface_config(&surface_id);
            }
        }
    }
}

impl Dispatch<WpFractionalScaleV1, String, State> for State {
    fn event(
        state: &mut State,
        _: &WpFractionalScaleV1,
        event: wp_fractional_scale_v1::Event,
        surface_id: &String,
        _: &Connection,
        _: &QueueHandle<State>,
    ) {
        let wp_fractional_scale_v1::Event::PreferredScale { scale } = event else {
            return;
        };
        // Clamp preferred_scale to 60..=480 (0.5x to 4.0x) to
        // prevent extreme values from a malicious compositor. Values outside
        // this range are silently ignored.
        let clamped = scale.clamp(60, 480);
        let new_scale = clamped as f32 / 120.0;
        if let Some(entry) = state.surfaces.get_mut(surface_id) {
            if (entry.scale - new_scale).abs() > f32::EPSILON {
                entry.scale = new_scale;
                entry.needs_full_redraw = true;
                tracing::info!(
                    scale = new_scale,
                    surface_id = surface_id.as_str(),
                    "wp_fractional_scale_v1: preferred_scale update triggered full redraw"
                );
            }
        }
    }
}

impl KeyboardHandler for State {
    fn enter(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        keyboard: &wl_keyboard::WlKeyboard,
        surface: &wl_surface::WlSurface,
        _serial: u32,
        _raw: &[u32],
        _keysyms: &[Keysym],
    ) {
        let Some(seat_id) = self.seat_id_for_keyboard(keyboard) else {
            return;
        };
        let focused = self.surface_id_for_wl_surface(surface);
        let release_grab = self.input_seats.get(&seat_id).is_some_and(|input| {
            if input.keyboard_focus != focused {
                // The previous focus is no longer eligible for repeats.
                // The mutable clear happens just below, outside this
                // predicate, to keep the seat lookup borrow short.
                true
            } else {
                false
            }
        });
        if release_grab {
            if let Some(input) = self.input_seats.get_mut(&seat_id) {
                input.keyboard_repeat = None;
            }
        }
        let grabbed = focused.as_ref().is_some_and(|surface_id| {
            self.input_seats
                .get(&seat_id)
                .and_then(|input| input.focus_grab_surface_id.as_deref())
                == Some(surface_id.as_ref())
        });
        if let Some(input) = self.input_seats.get_mut(&seat_id) {
            input.keyboard_focus = focused.clone();
        }
        if let Some(surface_id) = focused
            && grabbed
        {
            tracing::debug!(
                "[focus] layer_shell: keyboard focus entered grabbed surface_id={surface_id}; releasing focus grab"
            );
            self.release_surface_focus_grab_for_seat(&seat_id, &surface_id, true);
        }
    }

    fn leave(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        keyboard: &wl_keyboard::WlKeyboard,
        surface: &wl_surface::WlSurface,
        _serial: u32,
    ) {
        let Some(seat_id) = self.seat_id_for_keyboard(keyboard) else {
            return;
        };
        if let Some(surface_id) = self.surface_id_for_wl_surface(surface)
            && self
                .input_seats
                .get(&seat_id)
                .and_then(|input| input.focus_grab_surface_id.as_deref())
                == Some(surface_id.as_ref())
        {
            tracing::debug!(
                "[focus] layer_shell: keyboard focus left grabbed surface_id={surface_id}; releasing focus grab"
            );
            self.release_surface_focus_grab_for_seat(&seat_id, &surface_id, true);
        }
        if let Some(input) = self.input_seats.get_mut(&seat_id) {
            input.keyboard_focus = None;
            input.keyboard_repeat = None;
        }
    }

    fn press_key(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        keyboard: &wl_keyboard::WlKeyboard,
        _serial: u32,
        event: KeyEvent,
    ) {
        let Some(seat_id) = self.seat_id_for_keyboard(keyboard) else {
            return;
        };
        let Some(surface_id) = self
            .input_seats
            .get(&seat_id)
            .and_then(|input| input.keyboard_focus.clone())
        else {
            return;
        };
        let name = keysym_name(event.keysym);
        let mods = self
            .input_seats
            .get(&seat_id)
            .map_or_else(KeyMods::default, |input| KeyMods {
                ctrl: input.keyboard_mods.ctrl,
                shift: input.keyboard_mods.shift,
                alt: input.keyboard_mods.alt,
            });
        let text = committed_text(event.utf8.as_deref());
        let repeat = self.keyboard_repeat_state(
            &seat_id,
            &surface_id,
            name.as_ref(),
            mods.clone(),
            text.clone(),
            Instant::now(),
        );
        if let Some(input) = self.input_seats.get_mut(&seat_id) {
            input.keyboard_repeat = repeat;
        }
        let key_surface_id = surface_id.clone();
        self.queue_event(
            &seat_id,
            DevWindowEvent::Key {
                surface_id: key_surface_id,
                event: DevWindowKeyEvent::Pressed(name.into_owned(), mods),
            },
        );
        if let Some(text) = text {
            self.queue_event(&seat_id, DevWindowEvent::TextInput { surface_id, text });
        }
    }

    fn release_key(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        keyboard: &wl_keyboard::WlKeyboard,
        _serial: u32,
        event: KeyEvent,
    ) {
        let Some(seat_id) = self.seat_id_for_keyboard(keyboard) else {
            return;
        };
        let Some(surface_id) = self
            .input_seats
            .get(&seat_id)
            .and_then(|input| input.keyboard_focus.clone())
        else {
            return;
        };
        let name = keysym_name(event.keysym);
        self.clear_keyboard_repeat_for_key(&seat_id, name.as_ref());
        self.queue_event(
            &seat_id,
            DevWindowEvent::Key {
                surface_id,
                event: DevWindowKeyEvent::Released(name.into_owned()),
            },
        );
    }

    fn update_modifiers(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        keyboard: &wl_keyboard::WlKeyboard,
        _serial: u32,
        modifiers: Modifiers,
        _layout: u32,
    ) {
        let Some(seat_id) = self.seat_id_for_keyboard(keyboard) else {
            return;
        };
        if let Some(input) = self.input_seats.get_mut(&seat_id) {
            input.keyboard_mods = modifiers;
        }
        let mods = KeyMods {
            ctrl: modifiers.ctrl,
            shift: modifiers.shift,
            alt: modifiers.alt,
        };
        if let Some(input) = self.input_seats.get_mut(&seat_id)
            && let Some(repeat) = input.keyboard_repeat.as_mut()
        {
            repeat.mods = mods;
        }
    }

    fn update_repeat_info(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        keyboard: &wl_keyboard::WlKeyboard,
        info: RepeatInfo,
    ) {
        let Some(seat_id) = self.seat_id_for_keyboard(keyboard) else {
            return;
        };
        if let Some(input) = self.input_seats.get_mut(&seat_id) {
            input.keyboard_repeat_info = info;
        }
        if matches!(info, RepeatInfo::Disable) {
            if let Some(input) = self.input_seats.get_mut(&seat_id) {
                input.keyboard_repeat = None;
            }
        }
    }
}

fn keysym_name(sym: Keysym) -> Cow<'static, str> {
    sym.name()
        .map(normalize_keysym_name)
        .unwrap_or_else(|| Cow::Owned(format!("{:#x}", sym.raw())))
}

/// Preserve the compositor's complete committed payload. `KeyEvent::utf8`
/// can contain more than one scalar for composed input, and splitting it into
/// `Char` events would make one logical commit run the input/change handlers
/// several times with partial values.
fn committed_text(utf8: Option<&str>) -> Option<Arc<str>> {
    let text: String = utf8?.chars().filter(|ch| !ch.is_control()).collect();
    (!text.is_empty()).then(|| Arc::from(text))
}

fn normalize_keysym_name(name: &'static str) -> Cow<'static, str> {
    // `xkeysym::Keysym::name()` returns Rust-constant identifiers like `XK_Tab`.
    // Strip the prefix so downstream key matching sees the bare xkbcommon name.
    let name = name.strip_prefix("XK_").unwrap_or(name);
    match name {
        "Return" | "KP_Enter" => Cow::Borrowed("Enter"),
        "space" | "KP_Space" => Cow::Borrowed("Space"),
        "Tab" | "ISO_Left_Tab" => Cow::Borrowed("Tab"),
        "BackSpace" => Cow::Borrowed("Backspace"),
        "Left" | "KP_Left" => Cow::Borrowed("ArrowLeft"),
        "Right" | "KP_Right" => Cow::Borrowed("ArrowRight"),
        "Up" | "KP_Up" => Cow::Borrowed("ArrowUp"),
        "Down" | "KP_Down" => Cow::Borrowed("ArrowDown"),
        "Prior" => Cow::Borrowed("PageUp"),
        "Next" => Cow::Borrowed("PageDown"),
        "Escape" => Cow::Borrowed("Esc"),
        other => Cow::Borrowed(other),
    }
}

impl PopupHandler for State {
    fn configure(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        popup: &Popup,
        config: PopupConfigure,
    ) {
        let target = popup.wl_surface().clone();
        let Some(entry) = self
            .surfaces
            .values_mut()
            .find(|entry| entry.wl_surface() == &target)
        else {
            return;
        };
        // The compositor decides the popup's final size (it may have constrained
        // or resized the positioner request). Adopt it as the authoritative
        // logical size, exactly as the layer-shell `configure` path does.
        if config.width > 0 {
            entry.width = config.width as u32;
        }
        if config.height > 0 {
            entry.height = config.height as u32;
        }
        if let WaylandRole::Popup(role) = &mut entry.role {
            role.position = config.position;
            if let ConfigureKind::Reposition { token } = &config.kind {
                if role.pending_reposition_token != Some(*token) {
                    tracing::warn!(
                        expected_token = ?role.pending_reposition_token,
                        received_token = token,
                        "layer_shell: popup reposition configure token did not match the live request"
                    );
                } else {
                    role.pending_reposition_token = None;
                }
            }
        }
        entry.accept_configure();
        entry.needs_full_redraw = true;
    }

    fn done(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, popup: &Popup) {
        let dismissed = self.surface_id_for_wl_surface(popup.wl_surface());
        if let Some(id) = dismissed {
            tracing::debug!("[popover] layer_shell: compositor dismissed popup surface_id={id}");
            if self.teardown_surface_after_compositor_event(&id) {
                self.lifecycle_events
                    .push(SurfaceLifecycleEvent::Dismissed {
                        surface_id: id.to_string(),
                    });
            }
        }
    }
}

impl Dispatch<ZwpTextInputV3, (), State> for State {
    fn event(
        state: &mut State,
        text_input: &ZwpTextInputV3,
        event: zwp_text_input_v3::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<State>,
    ) {
        let Some(seat_id) = state.text_input_seats.get(&text_input.id()).cloned() else {
            return;
        };

        match event {
            zwp_text_input_v3::Event::Enter { surface } => {
                let Some(surface_id) = state.surface_id_for_wl_surface(&surface) else {
                    tracing::warn!("layer_shell: text-input-v3 entered an unknown surface");
                    return;
                };
                if let Some(input) = state.input_seats.get_mut(&seat_id) {
                    input.text_input_surface = Some(surface_id);
                    input.text_input_pending = PendingTextInput::default();
                }
                state.apply_text_input_state(&seat_id);
            }
            zwp_text_input_v3::Event::Leave { surface } => {
                let Some(surface_id) = state.surface_id_for_wl_surface(&surface) else {
                    return;
                };
                if let Some(input) = state.input_seats.get_mut(&seat_id)
                    && input.text_input_surface.as_deref() == Some(surface_id.as_ref())
                {
                    clear_text_input_for_surface(
                        surface_id.as_ref(),
                        &mut input.text_input_surface,
                        &mut input.text_input_pending,
                        &mut input.text_input_enabled,
                        &mut input.text_input_state_applied,
                    );
                }
            }
            zwp_text_input_v3::Event::PreeditString {
                text,
                cursor_begin,
                cursor_end,
            } => {
                if let Some(input) = state.input_seats.get_mut(&seat_id)
                    && input.text_input_surface.is_some()
                {
                    input.text_input_pending.preedit =
                        Some(text.map(|text| Arc::from(text.as_str())));
                    input.text_input_pending.preedit_cursor_begin = cursor_begin;
                    input.text_input_pending.preedit_cursor_end = cursor_end;
                }
            }
            zwp_text_input_v3::Event::CommitString { text } => {
                if let Some(input) = state.input_seats.get_mut(&seat_id)
                    && input.text_input_surface.is_some()
                {
                    input.text_input_pending.commit = text.map(|text| Arc::from(text.as_str()));
                }
            }
            zwp_text_input_v3::Event::DeleteSurroundingText {
                before_length,
                after_length,
            } => {
                if let Some(input) = state.input_seats.get_mut(&seat_id)
                    && input.text_input_surface.is_some()
                {
                    input.text_input_pending.before_bytes = before_length as usize;
                    input.text_input_pending.after_bytes = after_length as usize;
                }
            }
            zwp_text_input_v3::Event::Done { .. } => {
                let edit = state.input_seats.get_mut(&seat_id).and_then(|input| {
                    let surface_id = input.text_input_surface.clone()?;
                    input.text_input_pending.take_edit(surface_id)
                });
                if let Some(edit) = edit {
                    state.queue_event(&seat_id, edit);
                }
            }
            _ => {}
        }
    }
}

impl Dispatch<ZwpTextInputManagerV3, GlobalData> for State {
    fn event(
        _: &mut State,
        _: &ZwpTextInputManagerV3,
        _: zwp_text_input_manager_v3::Event,
        _: &GlobalData,
        _: &Connection,
        _: &QueueHandle<State>,
    ) {
        unreachable!("zwp_text_input_manager_v3 has no events");
    }
}

delegate_activation!(State);
delegate_compositor!(State);
delegate_output!(State);
delegate_shm!(State);
delegate_layer!(State);
delegate_seat!(State);
delegate_pointer!(State);
delegate_keyboard!(State);
delegate_registry!(State);
delegate_xdg_popup!(State);
// `xdg_surface` + `xdg_toplevel` for `role: "window"` surfaces.
delegate_xdg_window!(State);

// SCTK's `XdgShell` backs three things here: `xdg_wm_base` ping/pong, the
// positioner/popup factory, and the toplevel window factory. `delegate_xdg_shell!`
// would cover `xdg_wm_base` and the decoration manager together; we delegate them
// separately so the decoration-manager impl can stay documented in place.
wayland_client::delegate_dispatch!(State: [
    smithay_client_toolkit::reexports::protocols::xdg::shell::client::xdg_wm_base::XdgWmBase: smithay_client_toolkit::globals::GlobalData
] => smithay_client_toolkit::shell::xdg::XdgShell);

// Per-toplevel decoration objects, created by `XdgShell::create_window` when the
// compositor advertises the manager. `delegate_xdg_window!` does not cover them.
wayland_client::delegate_dispatch!(State: [
    wayland_protocols::xdg::decoration::zv1::client::zxdg_toplevel_decoration_v1::ZxdgToplevelDecorationV1: smithay_client_toolkit::shell::xdg::window::WindowData
] => smithay_client_toolkit::shell::xdg::XdgShell);

// `XdgShell::bind` also binds the optional `zxdg_decoration_manager_v1` global,
// which requires `State: Dispatch<ZxdgDecorationManagerV1, GlobalData>`. The
// manager itself is a pure factory with no events — the per-toplevel decoration
// objects above carry the mode negotiation.
impl Dispatch<ZxdgDecorationManagerV1, GlobalData> for State {
    fn event(
        _: &mut State,
        _: &ZxdgDecorationManagerV1,
        _: zxdg_decoration_manager_v1::Event,
        _: &GlobalData,
        _: &Connection,
        _: &QueueHandle<State>,
    ) {
        unreachable!("zxdg_decoration_manager_v1 has no events");
    }
}

impl Dispatch<WpViewport, ()> for State {
    fn event(
        _state: &mut Self,
        _proxy: &WpViewport,
        _event: <WpViewport as wayland_client::Proxy>::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        // wp_viewport has no events in protocol version 1.
    }
}

// The org_kde_kwin_blur_manager interface has no events — it is a factory
// that only creates org_kde_kwin_blur objects.
impl Dispatch<OrgKdeKwinBlurManager, GlobalData> for State {
    fn event(
        _: &mut State,
        _: &OrgKdeKwinBlurManager,
        _: org_kde_kwin_blur_manager::Event,
        _: &GlobalData,
        _: &Connection,
        _: &QueueHandle<State>,
    ) {
        unreachable!("org_kde_kwin_blur_manager has no events");
    }
}

// The org_kde_kwin_blur interface has no events — it is a pure request
// interface for set_region + commit.
impl Dispatch<OrgKdeKwinBlur, ()> for State {
    fn event(
        _: &mut State,
        _: &OrgKdeKwinBlur,
        _: org_kde_kwin_blur::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<State>,
    ) {
        unreachable!("org_kde_kwin_blur has no events");
    }
}

// zwp_pointer_gestures_v1 has no events — it is a factory for the swipe/
// pinch/hold gesture objects.
impl Dispatch<ZwpPointerGesturesV1, GlobalData> for State {
    fn event(
        _: &mut State,
        _: &ZwpPointerGesturesV1,
        _: zwp_pointer_gestures_v1::Event,
        _: &GlobalData,
        _: &Connection,
        _: &QueueHandle<State>,
    ) {
        unreachable!("zwp_pointer_gestures_v1 has no events");
    }
}

impl Dispatch<ZwpPointerGestureSwipeV1, GlobalData> for State {
    fn event(
        state: &mut State,
        gesture: &ZwpPointerGestureSwipeV1,
        event: zwp_pointer_gesture_swipe_v1::Event,
        _: &GlobalData,
        _: &Connection,
        _: &QueueHandle<State>,
    ) {
        let Some(seat_id) = state.seat_id_for_swipe(gesture) else {
            return;
        };
        match event {
            zwp_pointer_gesture_swipe_v1::Event::Begin {
                surface, fingers, ..
            } => {
                let Some(surface_id) = state.surface_id_for_wl_surface(&surface) else {
                    return;
                };
                if let Some(input) = state.input_seats.get_mut(&seat_id) {
                    input.gesture_surface = Some(surface_id.clone());
                    input.gesture_kind = Some(GestureKind::Swipe);
                }
                state.queue_event(
                    &seat_id,
                    DevWindowEvent::GestureSwipeBegin {
                        surface_id,
                        fingers,
                    },
                );
            }
            zwp_pointer_gesture_swipe_v1::Event::Update { dx, dy, .. } => {
                let Some(surface_id) = state
                    .input_seats
                    .get(&seat_id)
                    .and_then(|input| input.gesture_surface.clone())
                else {
                    return;
                };
                state.queue_event(
                    &seat_id,
                    DevWindowEvent::GestureSwipeUpdate {
                        surface_id,
                        dx: dx as f32,
                        dy: dy as f32,
                    },
                );
            }
            zwp_pointer_gesture_swipe_v1::Event::End { cancelled, .. } => {
                let Some(input) = state.input_seats.get_mut(&seat_id) else {
                    return;
                };
                let Some(surface_id) = input.gesture_surface.take() else {
                    return;
                };
                input.gesture_kind = None;
                state.queue_event(
                    &seat_id,
                    DevWindowEvent::GestureSwipeEnd {
                        surface_id,
                        cancelled: cancelled != 0,
                    },
                );
            }
            _ => {}
        }
    }
}

impl Dispatch<ZwpPointerGesturePinchV1, GlobalData> for State {
    fn event(
        state: &mut State,
        gesture: &ZwpPointerGesturePinchV1,
        event: zwp_pointer_gesture_pinch_v1::Event,
        _: &GlobalData,
        _: &Connection,
        _: &QueueHandle<State>,
    ) {
        let Some(seat_id) = state.seat_id_for_pinch(gesture) else {
            return;
        };
        match event {
            zwp_pointer_gesture_pinch_v1::Event::Begin {
                surface, fingers, ..
            } => {
                let Some(surface_id) = state.surface_id_for_wl_surface(&surface) else {
                    return;
                };
                if let Some(input) = state.input_seats.get_mut(&seat_id) {
                    input.gesture_surface = Some(surface_id.clone());
                    input.gesture_kind = Some(GestureKind::Pinch);
                }
                state.queue_event(
                    &seat_id,
                    DevWindowEvent::GesturePinchBegin {
                        surface_id,
                        fingers,
                    },
                );
            }
            zwp_pointer_gesture_pinch_v1::Event::Update {
                dx,
                dy,
                scale,
                rotation,
                ..
            } => {
                let Some(surface_id) = state
                    .input_seats
                    .get(&seat_id)
                    .and_then(|input| input.gesture_surface.clone())
                else {
                    return;
                };
                state.queue_event(
                    &seat_id,
                    DevWindowEvent::GesturePinchUpdate {
                        surface_id,
                        dx: dx as f32,
                        dy: dy as f32,
                        scale: scale as f32,
                        rotation: rotation as f32,
                    },
                );
            }
            zwp_pointer_gesture_pinch_v1::Event::End { cancelled, .. } => {
                let Some(input) = state.input_seats.get_mut(&seat_id) else {
                    return;
                };
                let Some(surface_id) = input.gesture_surface.take() else {
                    return;
                };
                input.gesture_kind = None;
                state.queue_event(
                    &seat_id,
                    DevWindowEvent::GesturePinchEnd {
                        surface_id,
                        cancelled: cancelled != 0,
                    },
                );
            }
            _ => {}
        }
    }
}

impl Dispatch<ZwpPointerGestureHoldV1, GlobalData> for State {
    fn event(
        state: &mut State,
        gesture: &ZwpPointerGestureHoldV1,
        event: zwp_pointer_gesture_hold_v1::Event,
        _: &GlobalData,
        _: &Connection,
        _: &QueueHandle<State>,
    ) {
        let Some(seat_id) = state.seat_id_for_hold(gesture) else {
            return;
        };
        match event {
            zwp_pointer_gesture_hold_v1::Event::Begin {
                surface, fingers, ..
            } => {
                let Some(surface_id) = state.surface_id_for_wl_surface(&surface) else {
                    return;
                };
                if let Some(input) = state.input_seats.get_mut(&seat_id) {
                    input.gesture_surface = Some(surface_id.clone());
                    input.gesture_kind = Some(GestureKind::Hold);
                }
                state.queue_event(
                    &seat_id,
                    DevWindowEvent::GestureHoldBegin {
                        surface_id,
                        fingers,
                    },
                );
            }
            zwp_pointer_gesture_hold_v1::Event::End { cancelled, .. } => {
                let Some(input) = state.input_seats.get_mut(&seat_id) else {
                    return;
                };
                let Some(surface_id) = input.gesture_surface.take() else {
                    return;
                };
                input.gesture_kind = None;
                state.queue_event(
                    &seat_id,
                    DevWindowEvent::GestureHoldEnd {
                        surface_id,
                        cancelled: cancelled != 0,
                    },
                );
            }
            _ => {}
        }
    }
}

impl TouchHandler for State {
    fn down(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        touch: &wl_touch::WlTouch,
        _serial: u32,
        _time: u32,
        surface: wl_surface::WlSurface,
        id: i32,
        position: (f64, f64),
    ) {
        let Some(seat_id) = self.seat_id_for_touch(touch) else {
            return;
        };
        let Some(surface_id) = self.surface_id_for_wl_surface(&surface) else {
            return;
        };
        if let Some(input) = self.input_seats.get_mut(&seat_id) {
            input.touch_surfaces.insert(id, surface_id.clone());
        }
        self.queue_event(
            &seat_id,
            DevWindowEvent::TouchDown {
                surface_id,
                id,
                x: position.0 as f32,
                y: position.1 as f32,
            },
        );
    }

    fn up(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        touch: &wl_touch::WlTouch,
        _serial: u32,
        _time: u32,
        id: i32,
    ) {
        let Some(seat_id) = self.seat_id_for_touch(touch) else {
            return;
        };
        let Some(surface_id) = self
            .input_seats
            .get_mut(&seat_id)
            .and_then(|input| input.touch_surfaces.remove(&id))
        else {
            return;
        };
        self.queue_event(&seat_id, DevWindowEvent::TouchUp { surface_id, id });
    }

    fn motion(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        touch: &wl_touch::WlTouch,
        _time: u32,
        id: i32,
        position: (f64, f64),
    ) {
        let Some(seat_id) = self.seat_id_for_touch(touch) else {
            return;
        };
        let Some(surface_id) = self
            .input_seats
            .get(&seat_id)
            .and_then(|input| input.touch_surfaces.get(&id).cloned())
        else {
            return;
        };
        self.queue_event(
            &seat_id,
            DevWindowEvent::TouchMove {
                surface_id,
                id,
                x: position.0 as f32,
                y: position.1 as f32,
            },
        );
    }

    fn shape(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _touch: &wl_touch::WlTouch,
        _id: i32,
        _major: f64,
        _minor: f64,
    ) {
        // Touch ellipse shape is not surfaced as a MESH event; no builtin
        // gesture currently needs contact-area precision.
    }

    fn orientation(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _touch: &wl_touch::WlTouch,
        _id: i32,
        _orientation: f64,
    ) {
        // Not surfaced; see `shape` above.
    }

    fn cancel(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, touch: &wl_touch::WlTouch) {
        let Some(seat_id) = self.seat_id_for_touch(touch) else {
            return;
        };
        self.cancel_touch_input_for_seat(&seat_id);
    }
}

delegate_touch!(State);

#[cfg(test)]
mod tests {
    use super::committed_text;
    use super::normalize_keysym_name;
    use super::normalized_axis_delta;

    #[test]
    fn axis_delta_prefers_continuous_motion_and_normalizes_direction() {
        assert_eq!(normalized_axis_delta(2.5, 1), -2.5);
        assert_eq!(normalized_axis_delta(-3.0, -1), 3.0);
    }

    #[test]
    fn axis_delta_preserves_step_only_frames() {
        assert_eq!(normalized_axis_delta(0.0, 1), -1.0);
        assert_eq!(normalized_axis_delta(0.0, -1), 1.0);
    }

    #[test]
    fn axis_delta_falls_back_when_continuous_motion_is_not_finite() {
        assert_eq!(normalized_axis_delta(f64::NAN, 1), -1.0);
    }
    use std::borrow::Cow;

    #[test]
    fn normalize_keysym_name_maps_common_xkb_names_to_shell_names() {
        assert_eq!(normalize_keysym_name("Return"), "Enter");
        assert_eq!(normalize_keysym_name("space"), "Space");
        assert_eq!(normalize_keysym_name("ISO_Left_Tab"), "Tab");
        assert_eq!(normalize_keysym_name("BackSpace"), "Backspace");
        assert_eq!(normalize_keysym_name("Escape"), "Esc");
        assert_eq!(normalize_keysym_name("Left"), "ArrowLeft");
        assert_eq!(normalize_keysym_name("Right"), "ArrowRight");
        assert_eq!(normalize_keysym_name("Up"), "ArrowUp");
        assert_eq!(normalize_keysym_name("Down"), "ArrowDown");
    }

    #[test]
    fn normalize_keysym_name_borrows_common_names() {
        assert!(matches!(
            normalize_keysym_name("Return"),
            Cow::Borrowed("Enter")
        ));
        assert!(matches!(normalize_keysym_name("XK_a"), Cow::Borrowed("a")));
    }

    #[test]
    fn committed_text_keeps_the_complete_unicode_payload() {
        assert_eq!(committed_text(Some("A🙂B")).as_deref(), Some("A🙂B"));
        assert_eq!(committed_text(Some("\n\t")).as_deref(), None);
    }
}
