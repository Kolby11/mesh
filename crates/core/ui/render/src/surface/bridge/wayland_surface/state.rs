use super::*;
use super::backend::{SurfaceEntry, apply_config};

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
    pub(super) events: Vec<DevWindowEvent>,
}

impl State {
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

impl ProvidesRegistryState for State {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry_state
    }

    registry_handlers![OutputState, SeatState];
}
