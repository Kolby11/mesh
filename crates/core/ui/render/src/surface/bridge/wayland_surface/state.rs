use super::backend::{SurfaceEntry, apply_config};
use super::*;

const MAX_REPEAT_EVENTS_PER_POLL: usize = 64;

pub(super) struct KeyboardRepeatState {
    pub(super) surface_id: String,
    pub(super) key: String,
    pub(super) mods: super::super::dev_window::KeyMods,
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
    pub(super) seat_state: SeatState,
    pub(super) activation_seat: Option<wl_seat::WlSeat>,
    pub(super) focus_grab: Option<HyprlandFocusGrabV1>,
    pub(super) focus_grab_surface_id: Option<String>,
    pub(super) qh: QueueHandle<State>,
    pub(super) pool: Option<SlotPool>,
    pub(super) surfaces: HashMap<String, SurfaceEntry>,
    pub(super) pointer: Option<wl_pointer::WlPointer>,
    pub(super) keyboard: Option<wl_keyboard::WlKeyboard>,
    pub(super) pointer_focus: Option<String>,
    pub(super) keyboard_focus: Option<String>,
    pub(super) keyboard_mods: Modifiers,
    pub(super) keyboard_repeat_info: RepeatInfo,
    pub(super) keyboard_repeat: Option<KeyboardRepeatState>,
    pub(super) events: Vec<DevWindowEvent>,
}

impl State {
    pub(super) fn schedule_keyboard_repeat(
        &mut self,
        surface_id: String,
        key: String,
        mods: super::super::dev_window::KeyMods,
        ch: Option<char>,
    ) {
        if is_non_repeating_key(&key) {
            self.keyboard_repeat = None;
            return;
        }

        let RepeatInfo::Repeat { rate, delay } = self.keyboard_repeat_info else {
            self.keyboard_repeat = None;
            return;
        };
        let interval = Duration::from_micros((1_000_000 / rate.get() as u64).max(1));
        self.keyboard_repeat = Some(KeyboardRepeatState {
            surface_id,
            key,
            mods,
            ch,
            next_at: Instant::now() + Duration::from_millis(delay as u64),
            interval,
        });
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

        let effective_cfg = entry.cfg.with_keyboard_mode(effective_keyboard_mode);
        tracing::debug!(
            "[focus] layer_shell: reapplying keyboard mode for surface_id={surface_id} mode={:?}",
            effective_keyboard_mode
        );
        apply_config(&entry.layer_surface, &effective_cfg);
        entry.layer_surface.commit();
        entry.applied_keyboard_mode = effective_keyboard_mode;
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
            grab.remove_surface(previous_entry.layer_surface.wl_surface());
        }

        if self.focus_grab_surface_id.as_deref() != Some(surface_id) {
            tracing::debug!("[focus] layer_shell: starting focus grab for surface_id={surface_id}");
            grab.add_surface(entry.layer_surface.wl_surface());
            grab.commit();
            self.focus_grab_surface_id = Some(surface_id.to_string());
            if let Some(previous_surface_id) = previous_surface_id.as_deref()
                && previous_surface_id != surface_id
            {
                self.reapply_surface_config(previous_surface_id);
            }
            self.reapply_surface_config(surface_id);
        }

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
                surface: Some(entry.layer_surface.wl_surface().clone()),
            },
        );
    }

    pub(super) fn release_surface_focus_grab(&mut self, surface_id: &str) {
        if self.focus_grab_surface_id.as_deref() != Some(surface_id) {
            return;
        }
        let Some(grab) = self.focus_grab.as_ref() else {
            self.focus_grab_surface_id = None;
            return;
        };
        if let Some(entry) = self.surfaces.get(surface_id) {
            tracing::debug!(
                "[focus] layer_shell: releasing focus grab for surface_id={surface_id}"
            );
            grab.remove_surface(entry.layer_surface.wl_surface());
            grab.commit();
        }
        self.focus_grab_surface_id = None;
        self.reapply_surface_config(surface_id);
    }

    pub(super) fn surface_id_for_wl_surface(
        &self,
        surface: &wl_surface::WlSurface,
    ) -> Option<String> {
        self.surfaces
            .iter()
            .find(|(_, entry)| entry.layer_surface.wl_surface() == surface)
            .map(|(surface_id, _)| surface_id.clone())
    }
}

fn is_non_repeating_key(key: &str) -> bool {
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

impl ProvidesRegistryState for State {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry_state
    }

    registry_handlers![OutputState, SeatState];
}
