use super::backend::{SurfaceEntry, apply_config, surface_config_fingerprint};
use super::*;

const MAX_REPEAT_EVENTS_PER_POLL: usize = 64;
const SURFACE_FOCUS_GRAB_TIMEOUT: Duration = Duration::from_millis(750);

pub(super) struct KeyboardRepeatState {
    pub(super) surface_id: String,
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
    pub(super) focus_grab_surface_id: Option<String>,
    pub(super) focus_grab_requested_at: Option<Instant>,
    pub(super) qh: QueueHandle<State>,
    pub(super) pool: Option<SlotPool>,
    pub(super) surfaces: HashMap<String, SurfaceEntry>,
    pub(super) surface_ids_by_wl_id: HashMap<ObjectId, String>,
    pub(super) pointer: Option<ThemedPointer>,
    pub(super) pointer_interactive: bool,
    pub(super) keyboard: Option<wl_keyboard::WlKeyboard>,
    pub(super) pointer_focus: Option<String>,
    pub(super) keyboard_focus: Option<String>,
    pub(super) keyboard_mods: Modifiers,
    pub(super) keyboard_repeat_info: RepeatInfo,
    pub(super) keyboard_repeat: Option<KeyboardRepeatState>,
    pub(super) events: Vec<DevWindowEvent>,
    /// `xdg_shell` (`xdg_wm_base`) global, bound when available. Required to
    /// create `xdg_positioner`/`xdg_popup` objects for promoted `<popover>`s.
    pub(super) xdg_shell: Option<XdgShell>,
    /// `surface_id`s of popups the compositor dismissed (e.g. outside-click on a
    /// grabbed popup, or parent surface destroyed). Drained by the shell so it
    /// can drop the matching popup target. Entries are removed from `surfaces`
    /// immediately; this only carries the id outward.
    pub(super) dismissed_popups: Vec<String>,
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
            self.focus_grab_surface_id = Some(surface_id.to_string());
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
            if keyboard_focus != surface_id {
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
        if self.focus_grab_surface_id.as_deref() != Some(surface_id) {
            return;
        }
        let Some(grab) = self.focus_grab.take() else {
            self.focus_grab_surface_id = None;
            self.focus_grab_requested_at = None;
            self.reapply_surface_config(surface_id);
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
        self.reapply_surface_config(surface_id);
    }

    pub(super) fn insert_surface(&mut self, surface_id: String, entry: SurfaceEntry) {
        let wl_id = entry.wl_surface().id();
        if let Some(previous) = self.surfaces.insert(surface_id.clone(), entry) {
            self.surface_ids_by_wl_id
                .remove(&previous.wl_surface().id());
        }
        self.surface_ids_by_wl_id.insert(wl_id, surface_id);
    }

    pub(super) fn remove_surface(&mut self, surface_id: &str) -> Option<SurfaceEntry> {
        let entry = self.surfaces.remove(surface_id)?;
        self.surface_ids_by_wl_id.remove(&entry.wl_surface().id());
        Some(entry)
    }

    pub(super) fn surface_id_for_wl_surface(
        &self,
        surface: &wl_surface::WlSurface,
    ) -> Option<String> {
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
        surface_id: surface_id.to_string(),
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

        let repeat =
            keyboard_repeat_state_for(repeat_info, "panel", "a", mods, Some('a'), now).unwrap();
        assert_eq!(repeat.surface_id, "panel");
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
                surface_id: surface_id.clone(),
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
                surface_id: key_surface_id,
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
                surface_id: surface_id.to_string(),
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

impl ProvidesRegistryState for State {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry_state
    }

    registry_handlers![OutputState, SeatState];
}
