use super::*;

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
        // T-102-01: Clamp to 1..=3 to prevent extreme scale values from
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
            entry.frame_pending = false;
            entry.frame_pending_since = None;
        }
    }

    fn surface_enter(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &wl_surface::WlSurface,
        _output: &wl_output::WlOutput,
    ) {
    }

    fn surface_leave(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &wl_surface::WlSurface,
        _output: &wl_output::WlOutput,
    ) {
    }
}

impl OutputHandler for State {
    fn output_state(&mut self) -> &mut OutputState {
        &mut self.output_state
    }

    fn new_output(&mut self, _c: &Connection, _q: &QueueHandle<Self>, _o: wl_output::WlOutput) {}

    fn update_output(&mut self, _c: &Connection, _q: &QueueHandle<Self>, _o: wl_output::WlOutput) {}

    fn output_destroyed(
        &mut self,
        _c: &Connection,
        _q: &QueueHandle<Self>,
        _o: wl_output::WlOutput,
    ) {
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
            self.release_surface_focus_grab(&id);
            self.remove_surface(&id);
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
            entry.configured = true;
        }
    }
}

impl SeatHandler for State {
    fn seat_state(&mut self) -> &mut SeatState {
        &mut self.seat_state
    }

    fn new_seat(&mut self, _c: &Connection, _q: &QueueHandle<Self>, _s: wl_seat::WlSeat) {}

    fn new_capability(
        &mut self,
        _conn: &Connection,
        qh: &QueueHandle<Self>,
        seat: wl_seat::WlSeat,
        capability: SeatCapability,
    ) {
        self.activation_seat = Some(seat.clone());
        if capability == SeatCapability::Pointer && self.pointer.is_none() {
            let cursor_surface = self.compositor_state.create_surface(qh);
            if let Ok(ptr) = self.seat_state.get_pointer_with_theme(
                qh,
                &seat,
                self.shm.wl_shm(),
                cursor_surface,
                ThemeSpec::default(),
            ) {
                tracing::debug!("[hover] layer_shell: pointer capability acquired");
                self.pointer = Some(ptr);
            }
        }
        if capability == SeatCapability::Keyboard
            && self.keyboard.is_none()
            && let Ok(kbd) = self.seat_state.get_keyboard(qh, &seat, None)
        {
            self.keyboard = Some(kbd);
        }
    }

    fn remove_capability(
        &mut self,
        _c: &Connection,
        _q: &QueueHandle<Self>,
        _s: wl_seat::WlSeat,
        capability: SeatCapability,
    ) {
        if capability == SeatCapability::Pointer {
            let _ = self.pointer.take();
        }
        if capability == SeatCapability::Keyboard
            && let Some(keyboard) = self.keyboard.take()
        {
            keyboard.release();
        }
    }

    fn remove_seat(&mut self, _c: &Connection, _q: &QueueHandle<Self>, seat: wl_seat::WlSeat) {
        if self.activation_seat.as_ref() == Some(&seat) {
            self.activation_seat = None;
        }
    }
}

impl PointerHandler for State {
    fn pointer_frame(
        &mut self,
        conn: &Connection,
        _qh: &QueueHandle<Self>,
        _pointer: &wl_pointer::WlPointer,
        events: &[PointerEvent],
    ) {
        for event in events {
            let surface_id = match self.surface_id_for_wl_surface(&event.surface) {
                Some(id) => id,
                None => continue,
            };
            match event.kind {
                PointerEventKind::Enter { .. } => {
                    tracing::debug!("[hover] layer_shell: pointer enter surface_id={surface_id}");
                    self.pointer_focus = Some(surface_id.clone());
                    if let Some(pointer) = self.pointer.as_ref()
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
                    self.events
                        .push(DevWindowEvent::PointerMove { surface_id, x, y });
                }
                PointerEventKind::Leave { .. } => {
                    tracing::debug!("[hover] layer_shell: pointer leave surface_id={surface_id}");
                    if self.focus_grab_surface_id.as_deref() == Some(surface_id.as_str()) {
                        tracing::debug!(
                            "[focus] layer_shell: pointer left grabbed surface_id={surface_id}; releasing focus grab"
                        );
                        self.release_surface_focus_grab(&surface_id);
                    }
                    if self.pointer_focus.as_deref() == Some(&surface_id) {
                        self.pointer_focus = None;
                    }
                    self.events
                        .push(DevWindowEvent::PointerLeave { surface_id });
                }
                PointerEventKind::Motion { .. } => {
                    let (x, y) = (event.position.0 as f32, event.position.1 as f32);
                    tracing::trace!(
                        "[hover] layer_shell: pointer motion surface_id={surface_id} x={x:.1} y={y:.1}"
                    );
                    self.events
                        .push(DevWindowEvent::PointerMove { surface_id, x, y });
                }
                PointerEventKind::Press { button, .. } => {
                    if button == 0x110 {
                        self.request_surface_focus(&surface_id, event);
                        let (x, y) = (event.position.0 as f32, event.position.1 as f32);
                        tracing::debug!(
                            "[hover] layer_shell: pointer press surface_id={surface_id} x={x:.1} y={y:.1}"
                        );
                        self.events.push(DevWindowEvent::PointerButton {
                            surface_id,
                            x,
                            y,
                            pressed: true,
                        });
                    }
                }
                PointerEventKind::Release { button, .. } => {
                    if button == 0x110 {
                        let (x, y) = (event.position.0 as f32, event.position.1 as f32);
                        tracing::debug!(
                            "[hover] layer_shell: pointer release surface_id={surface_id} x={x:.1} y={y:.1}"
                        );
                        self.events.push(DevWindowEvent::PointerButton {
                            surface_id,
                            x,
                            y,
                            pressed: false,
                        });
                    }
                }
                PointerEventKind::Axis {
                    horizontal,
                    vertical,
                    ..
                } => {
                    let (x, y) = (event.position.0 as f32, event.position.1 as f32);
                    let dx = -horizontal.absolute as f32;
                    let dy = -vertical.absolute as f32;
                    if dx.abs() > f32::EPSILON || dy.abs() > f32::EPSILON {
                        self.events.push(DevWindowEvent::Scroll {
                            surface_id,
                            x,
                            y,
                            dx,
                            dy,
                        });
                    }
                }
            }
        }
    }
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
        _: &HyprlandFocusGrabV1,
        event: hyprland_focus_grab_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<State>,
    ) {
        if let hyprland_focus_grab_v1::Event::Cleared = event {
            tracing::debug!("[focus] layer_shell: compositor cleared focus grab");
            if let Some(grab) = state.focus_grab.take() {
                grab.destroy();
            }
            state.focus_grab_requested_at = None;
            if let Some(surface_id) = state.focus_grab_surface_id.take() {
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
        // T-102-02: Clamp preferred_scale to 60..=480 (0.5x to 4.0x) to
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
        _keyboard: &wl_keyboard::WlKeyboard,
        surface: &wl_surface::WlSurface,
        _serial: u32,
        _raw: &[u32],
        _keysyms: &[Keysym],
    ) {
        let focused = self.surface_id_for_wl_surface(surface);
        if self.keyboard_focus != focused {
            self.keyboard_repeat = None;
        }
        self.keyboard_focus = focused.clone();
        if let Some(surface_id) = focused
            && self.focus_grab_surface_id.as_deref() == Some(surface_id.as_str())
        {
            tracing::debug!(
                "[focus] layer_shell: keyboard focus entered grabbed surface_id={surface_id}; releasing focus grab"
            );
            self.release_surface_focus_grab(&surface_id);
        }
    }

    fn leave(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _keyboard: &wl_keyboard::WlKeyboard,
        surface: &wl_surface::WlSurface,
        _serial: u32,
    ) {
        if let Some(surface_id) = self.surface_id_for_wl_surface(surface)
            && self.focus_grab_surface_id.as_deref() == Some(surface_id.as_str())
        {
            tracing::debug!(
                "[focus] layer_shell: keyboard focus left grabbed surface_id={surface_id}; releasing focus grab"
            );
            self.release_surface_focus_grab(&surface_id);
        }
        self.keyboard_focus = None;
        self.keyboard_repeat = None;
    }

    fn press_key(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _keyboard: &wl_keyboard::WlKeyboard,
        _serial: u32,
        event: KeyEvent,
    ) {
        let Some(mut surface_id) = self.keyboard_focus.clone() else {
            return;
        };
        let name = keysym_name(event.keysym);
        let mods = KeyMods {
            ctrl: self.keyboard_mods.ctrl,
            shift: self.keyboard_mods.shift,
            alt: self.keyboard_mods.alt,
        };
        let ch = event
            .utf8
            .as_deref()
            .and_then(|s| s.chars().next())
            .filter(|ch| !ch.is_control());
        self.keyboard_repeat =
            self.keyboard_repeat_state(&surface_id, &name, mods.clone(), ch, Instant::now());
        let key_surface_id = if ch.is_some() || self.keyboard_repeat.is_some() {
            surface_id.clone()
        } else {
            std::mem::take(&mut surface_id)
        };
        self.events.push(DevWindowEvent::Key {
            surface_id: key_surface_id,
            event: DevWindowKeyEvent::Pressed(name, mods),
        });
        if let Some(ch) = ch {
            self.events.push(DevWindowEvent::Char { surface_id, ch });
        }
    }

    fn release_key(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _keyboard: &wl_keyboard::WlKeyboard,
        _serial: u32,
        event: KeyEvent,
    ) {
        let Some(surface_id) = self.keyboard_focus.clone() else {
            return;
        };
        let name = keysym_name(event.keysym);
        self.clear_keyboard_repeat_for_key(&name);
        self.events.push(DevWindowEvent::Key {
            surface_id,
            event: DevWindowKeyEvent::Released(name),
        });
    }

    fn update_modifiers(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _keyboard: &wl_keyboard::WlKeyboard,
        _serial: u32,
        modifiers: Modifiers,
        _layout: u32,
    ) {
        self.keyboard_mods = modifiers;
        let mods = KeyMods {
            ctrl: self.keyboard_mods.ctrl,
            shift: self.keyboard_mods.shift,
            alt: self.keyboard_mods.alt,
        };
        if let Some(repeat) = self.keyboard_repeat.as_mut() {
            repeat.mods = mods;
        }
    }

    fn update_repeat_info(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _keyboard: &wl_keyboard::WlKeyboard,
        info: RepeatInfo,
    ) {
        self.keyboard_repeat_info = info;
        if matches!(info, RepeatInfo::Disable) {
            self.keyboard_repeat = None;
        }
    }
}

fn keysym_name(sym: Keysym) -> String {
    sym.name()
        .map(normalize_keysym_name)
        .unwrap_or_else(|| format!("{:#x}", sym.raw()))
}

fn normalize_keysym_name(name: &str) -> String {
    // `xkeysym::Keysym::name()` returns Rust-constant identifiers like `XK_Tab`.
    // Strip the prefix so downstream key matching sees the bare xkbcommon name.
    let name = name.strip_prefix("XK_").unwrap_or(name);
    match name {
        "Return" | "KP_Enter" => "Enter".into(),
        "space" | "KP_Space" => "Space".into(),
        "Tab" | "ISO_Left_Tab" => "Tab".into(),
        "BackSpace" => "Backspace".into(),
        "Left" | "KP_Left" => "ArrowLeft".into(),
        "Right" | "KP_Right" => "ArrowRight".into(),
        "Up" | "KP_Up" => "ArrowUp".into(),
        "Down" | "KP_Down" => "ArrowDown".into(),
        "Prior" => "PageUp".into(),
        "Next" => "PageDown".into(),
        "Escape" => "Esc".into(),
        other => other.to_string(),
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
        entry.configured = true;
        entry.needs_full_redraw = true;
    }

    fn done(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, popup: &Popup) {
        let dismissed = self.surface_id_for_wl_surface(popup.wl_surface());
        if let Some(id) = dismissed {
            tracing::debug!("[popover] layer_shell: compositor dismissed popup surface_id={id}");
            self.remove_surface(&id);
            self.dismissed_popups.push(id);
        }
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

// We use SCTK's `XdgShell` only for `xdg_wm_base` ping/pong and the
// positioner/popup factory — not for toplevel windows. `delegate_xdg_shell!`
// would also require a `WindowHandler` (for server-side window decorations),
// so instead delegate just the two globals `XdgShell` needs to dispatch:
// `xdg_wm_base` itself and the optional decoration manager bound by `bind()`.
wayland_client::delegate_dispatch!(State: [
    smithay_client_toolkit::reexports::protocols::xdg::shell::client::xdg_wm_base::XdgWmBase: smithay_client_toolkit::globals::GlobalData
] => smithay_client_toolkit::shell::xdg::XdgShell);

// `XdgShell::bind` also binds the optional `zxdg_decoration_manager_v1` global,
// which requires `State: Dispatch<ZxdgDecorationManagerV1, GlobalData>`. We never
// use server-side decorations, so rather than pull in `WindowHandler` via
// `delegate_xdg_shell!`, handle the manager directly — it is a pure factory with
// no events.
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

#[cfg(test)]
mod tests {
    use super::normalize_keysym_name;

    #[test]
    fn normalize_keysym_name_maps_common_xkb_names_to_shell_names() {
        assert_eq!(normalize_keysym_name("Return"), "Enter");
        assert_eq!(normalize_keysym_name("space"), "Space");
        assert_eq!(normalize_keysym_name("ISO_Left_Tab"), "Tab");
        assert_eq!(normalize_keysym_name("BackSpace"), "Backspace");
        assert_eq!(normalize_keysym_name("Left"), "ArrowLeft");
        assert_eq!(normalize_keysym_name("Right"), "ArrowRight");
        assert_eq!(normalize_keysym_name("Up"), "ArrowUp");
        assert_eq!(normalize_keysym_name("Down"), "ArrowDown");
    }
}
