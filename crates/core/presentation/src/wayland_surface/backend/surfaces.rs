use super::*;

impl WaylandSurfaceBackend {
    pub fn new() -> Result<Self, PresentationError> {
        let conn = Connection::connect_to_env()
            .map_err(|e| PresentationError::WaylandConnect(format!("connect_to_env: {e}")))?;
        let (globals, event_queue) = registry_queue_init::<State>(&conn)
            .map_err(|e| PresentationError::WaylandConnect(format!("registry_queue_init: {e}")))?;
        let qh = event_queue.handle();

        let registry_state = RegistryState::new(&globals);
        let output_state = OutputState::new(&globals, &qh);
        let compositor_state = CompositorState::bind(&globals, &qh)
            .map_err(|e| PresentationError::ProtocolUnsupported(format!("wl_compositor: {e}")))?;
        let shm = Shm::bind(&globals, &qh)
            .map_err(|e| PresentationError::ProtocolUnsupported(format!("wl_shm: {e}")))?;
        let layer_shell = LayerShell::bind(&globals, &qh).map_err(|e| {
            PresentationError::ProtocolUnsupported(format!("zwlr_layer_shell_v1: {e}"))
        })?;
        // xdg_wm_base backs the popup positioner primitive. It is optional: a
        // compositor without it simply cannot promote `<popover>`s (the shell
        // falls back to keeping them inline / clipped), so a missing global is
        // not a hard failure for the rest of the shell.
        let xdg_shell = XdgShell::bind(&globals, &qh).ok();
        if xdg_shell.is_none() {
            tracing::warn!(
                "layer_shell: xdg_wm_base unavailable; <popover> surface promotion disabled"
            );
        }
        let activation_state = ActivationState::bind(&globals, &qh).ok();
        let focus_grab_manager = globals.bind(&qh, 1..=1, GlobalData).ok();
        let viewporter: Option<WpViewporter> = globals.bind(&qh, 1..=1, GlobalData).ok();
        let fractional_scale_manager: Option<WpFractionalScaleManagerV1> =
            globals.bind(&qh, 1..=1, GlobalData).ok();
        let blur_manager: Option<OrgKdeKwinBlurManager> = globals.bind(&qh, 1..=1, GlobalData).ok();
        // Trackpad gesture support (two-finger scroll is plain wl_pointer axis
        // events, already handled elsewhere; this covers swipe/pinch/hold).
        // Optional: compositors without it just never emit gesture events.
        let pointer_gestures: Option<ZwpPointerGesturesV1> =
            globals.bind(&qh, 1..=3, GlobalData).ok();
        let seat_state = SeatState::new(&globals, &qh);

        let pool = SlotPool::new(256 * 256 * 4, &shm).ok();

        let state = State {
            registry_state,
            output_state,
            compositor_state,
            shm,
            layer_shell,
            activation_state,
            focus_grab_manager,
            viewporter,
            fractional_scale_manager,
            blur_manager,
            pointer_gestures,
            seat_state,
            activation_seat: None,
            qh,
            pool,
            surfaces: HashMap::new(),
            surface_ids_by_wl_id: HashMap::new(),
            pointer_interactive: false,
            input_seats: HashMap::new(),
            input_seat_order: Vec::new(),
            pointer_seats: HashMap::new(),
            keyboard_seats: HashMap::new(),
            touch_seats: HashMap::new(),
            swipe_seats: HashMap::new(),
            pinch_seats: HashMap::new(),
            hold_seats: HashMap::new(),
            focus_grab_seats: HashMap::new(),
            events: Vec::new(),
            xdg_shell,
            lifecycle_events: Vec::new(),
            close_requests: Vec::new(),
            connection_lost: None,
        };

        Ok(Self {
            _conn: conn,
            event_queue,
            state,
        })
    }

    pub fn set_pointer_interactive(&mut self, interactive: bool) {
        if self.state.pointer_interactive == interactive {
            return;
        }
        self.state.pointer_interactive = interactive;
        let icon = if interactive {
            CursorIcon::Pointer
        } else {
            CursorIcon::Default
        };
        for seat in self.state.input_seats.values() {
            if let Some(pointer) = seat.pointer.as_ref()
                && let Err(error) = pointer.set_cursor(&self._conn, icon)
            {
                tracing::debug!("layer_shell: failed to update cursor icon: {error}");
            }
        }
    }

    /// Apply a surface's desired config. Creates the compositor object — a
    /// layer surface or an `xdg_toplevel`, per [`SurfaceConfig::role`] — lazily
    /// on first call.
    ///
    /// A role change on a live surface tears the old compositor object down and
    /// recreates it under the same `surface_id`. Everything above the
    /// presentation layer (component VM, retained tree, Lua state, service
    /// subscriptions) is untouched: only the Wayland object is swapped, which is
    /// what makes promoting a widget into a window non-destructive.
    pub fn configure(
        &mut self,
        surface_id: &str,
        cfg: SurfaceConfig,
    ) -> Result<(), PresentationError> {
        if let Some(error) = self.state.connection_lost_error() {
            return Err(error);
        }
        let cfg = self.clamp_surface_config(surface_id, cfg);
        let qh = self.state.qh.clone();
        let effective_keyboard_mode = self
            .state
            .effective_keyboard_mode_for(surface_id, cfg.keyboard_mode);

        let must_recreate = self.state.surfaces.get(surface_id).is_some_and(|entry| {
            entry
                .config_change(&cfg, effective_keyboard_mode)
                .requires_recreation()
        });
        if must_recreate {
            tracing::info!(
                surface_id,
                role = ?cfg.role,
                "surface creation identity changed; recreating the compositor surface"
            );
            // Create the replacement while the last-known-good role is still
            // installed. A missing protocol global or compositor allocation
            // failure must leave the existing surface usable for the next
            // retry, rather than turning a failed configure into a teardown.
            let role = self.create_surface_role(&cfg, &qh)?;
            if cfg.keyboard_mode != KeyboardMode::OnDemand {
                self.state.release_surface_focus_grab(surface_id);
            }
            self.destroy_surface(surface_id);
            self.install_surface_role(surface_id, role, cfg, effective_keyboard_mode);
            return Ok(());
        }

        if cfg.keyboard_mode != KeyboardMode::OnDemand {
            self.state.release_surface_focus_grab(surface_id);
        }

        match self.state.surfaces.get_mut(surface_id) {
            Some(entry) => {
                if entry.needs_reconfigure(&cfg, effective_keyboard_mode) {
                    // Re-commit to re-map the surface and prompt the compositor to
                    // send a fresh configure event before we attach a buffer.
                    entry.apply_config(cfg, effective_keyboard_mode);
                } else {
                    // Identical config: the reserve is identical too, but assign
                    // it anyway so the entry's padding can never drift from the
                    // last config the shell sent.
                    entry.padding = cfg.padding;
                }
            }
            None => {
                let role = self.create_surface_role(&cfg, &qh)?;
                self.install_surface_role(surface_id, role, cfg, effective_keyboard_mode);
            }
        }
        Ok(())
    }

    fn install_surface_role(
        &mut self,
        surface_id: &str,
        role: WaylandRole,
        cfg: SurfaceConfig,
        effective_keyboard_mode: KeyboardMode,
    ) {
        self.state.insert_surface(
            surface_id.to_string(),
            SurfaceEntry::new(role, cfg, effective_keyboard_mode),
        );
        if let Some(entry) = self.state.surfaces.get_mut(surface_id) {
            let cfg = entry.cfg.clone();
            entry.apply_config(cfg, effective_keyboard_mode);
        }
        let wl_surface = self
            .state
            .surfaces
            .get(surface_id)
            .map(|entry| entry.wl_surface().clone());
        let qh = self.state.qh.clone();
        if let Some(wl_surface) = wl_surface
            && let Some(fs) =
                self.state
                    .bind_fractional_scale(&wl_surface, &qh, surface_id.to_string())
            && let Some(entry) = self.state.surfaces.get_mut(surface_id)
        {
            entry.fractional_scale = Some(fs);
        }
        if let Some(ref viewporter) = self.state.viewporter {
            if let Some(entry) = self.state.surfaces.get_mut(surface_id) {
                let wl_surface = entry.wl_surface().clone();
                let qh = self.state.qh.clone();
                entry.viewport = Some(viewporter.get_viewport(&wl_surface, &qh, ()));
            }
        }
        // The kde_blur object is created lazily when the first non-empty blur
        // region is committed in `present_with_damage`. Creating it eagerly
        // would enable the compositor's default whole-surface blur on every
        // surface — including ones with no `backdrop-filter` at all — because
        // an org_kde_kwin_blur object with no region set blurs the entire
        // surface.
    }

    /// Create the compositor object backing a new surface.
    ///
    /// A layer surface can always be created — `zwlr_layer_shell_v1` is a hard
    /// requirement of the backend. A window needs `xdg_wm_base`, which is
    /// optional here (see [`WaylandSurfaceBackend::new`]), so a window request
    /// on a compositor without it is a surface-creation error rather than a
    /// silent downgrade to shell chrome: a settings window quietly reappearing
    /// as a panel across the top of the screen is worse than a diagnostic.
    fn create_surface_role(
        &mut self,
        cfg: &SurfaceConfig,
        qh: &QueueHandle<State>,
    ) -> Result<WaylandRole, PresentationError> {
        if cfg.role == SurfaceRole::Window {
            let xdg_shell = self.state.xdg_shell.as_ref().ok_or_else(|| {
                PresentationError::ProtocolUnsupported(
                    "xdg_wm_base unavailable; cannot create a window surface".into(),
                )
            })?;
            let surface = Surface::new(&self.state.compositor_state, qh)
                .map_err(|e| PresentationError::SurfaceCreate(format!("window surface: {e}")))?;
            let window = xdg_shell.create_window(
                surface,
                map_window_decorations(cfg.window.decorations),
                qh,
            );
            // The initial commit with no buffer maps the toplevel role and
            // prompts the compositor's first configure. Until that arrives the
            // present gate holds every frame back, exactly as for a layer
            // surface.
            window.commit();
            return Ok(WaylandRole::Window(WindowRole {
                window,
                compositor_size: None,
                applied_size_hints: None,
                applied_geometry: None,
                states: WindowStates::default(),
            }));
        }

        let wl_surface = self.state.compositor_state.create_surface(qh);
        Ok(WaylandRole::Layer(
            self.state.layer_shell.create_layer_surface(
                qh,
                wl_surface,
                map_layer(cfg.layer),
                Some(cfg.wayland_namespace()),
                None,
            ),
        ))
    }

    /// The size the compositor last configured for a window surface, when it
    /// named one. `None` for layer surfaces and popups (whose sizes the client
    /// or the positioner decides), and for a window that has not yet received a
    /// sized configure.
    pub fn window_configured_size(&self, surface_id: &str) -> Option<(u32, u32)> {
        match &self.state.surfaces.get(surface_id)?.role {
            WaylandRole::Window(role) => role.compositor_size,
            _ => None,
        }
    }

    /// The `xdg_toplevel` states from the window's last configure. Layer
    /// surfaces and popups have no such states and report the default (all
    /// false), which is also what a window reports before its first configure.
    pub fn window_states(&self, surface_id: &str) -> WindowStates {
        match self.state.surfaces.get(surface_id).map(|entry| &entry.role) {
            Some(WaylandRole::Window(role)) => role.states,
            _ => WindowStates::default(),
        }
    }

    /// Drain the ids of windows the user asked to close (title-bar button,
    /// compositor keybind). The shell decides what closing means — hiding the
    /// surface, running a teardown handler — so this only reports the request.
    pub fn take_close_requests(&mut self) -> Vec<String> {
        std::mem::take(&mut self.state.close_requests)
    }

    /// True when the compositor exposes `xdg_wm_base`, i.e. `<popover>` surface
    /// promotion is available.
    pub fn popup_supported(&self) -> bool {
        self.state.xdg_shell.is_some()
    }

    /// True when the compositor exposes `xdg_wm_base`, i.e. a surface can be
    /// realized as an `xdg_toplevel`. Same global as [`Self::popup_supported`];
    /// see `PresentationEngine::window_role_supported` for why it is asked
    /// separately.
    pub fn window_role_supported(&self) -> bool {
        self.state.xdg_shell.is_some()
    }

    /// Promote a component into an `xdg_popup` child of an existing layer
    /// surface, or reposition it if `surface_id` already names a live popup.
    ///
    /// The popup shares the entire SHM/present/scale/input path with layer
    /// surfaces — only creation and placement differ. Like a layer surface it
    /// must not be painted until the compositor sends its first configure, which
    /// flips `configured = true` via [`PopupHandler::configure`]; the existing
    /// `present_with_damage` gate handles that automatically.
    pub fn configure_popup(
        &mut self,
        surface_id: &str,
        config: PopupConfig,
    ) -> Result<(), PresentationError> {
        if let Some(error) = self.state.connection_lost_error() {
            return Err(error);
        }
        // An existing popup is repositioned in place rather than recreated, so
        // anchor moves (exclusive-zone/output changes) don't tear it down. The
        // shadow/overshoot reserve travels with the placement size, so adopt it
        // on the same path that adopts the new size.
        if let Some(entry) = self.state.surfaces.get_mut(surface_id) {
            entry.padding = config.padding;
            self.reposition_popup(surface_id, &config.placement);
            return Ok(());
        }

        if self.state.xdg_shell.is_none() {
            return Err(PresentationError::ProtocolUnsupported(
                "xdg_wm_base unavailable; cannot promote popover".into(),
            ));
        }

        // The popup's Wayland parent is either a layer surface or a window, and
        // the two are parented by different protocol calls: a layer surface
        // adopts the popup afterwards via `zwlr_layer_surface_v1.get_popup`,
        // while a toplevel is named as the parent up-front in
        // `xdg_surface.get_popup`. Nested popup-of-popup is not modeled yet.
        let parent = match self.state.surfaces.get(&config.parent_surface_id) {
            Some(entry) => match (
                entry.role.as_layer(),
                entry.role.as_popup_parent_xdg_surface(),
            ) {
                (Some(layer), _) => PopupParent::Layer(layer.clone()),
                (None, Some(xdg_surface)) => PopupParent::Window(xdg_surface.clone()),
                (None, None) => {
                    return Err(PresentationError::SurfaceCreate(
                        "popup parent must be a layer surface or a window".into(),
                    ));
                }
            },
            None => {
                return Err(PresentationError::SurfaceCreate(format!(
                    "popup parent surface '{}' not found",
                    config.parent_surface_id
                )));
            }
        };

        let qh = self.state.qh.clone();
        // `XdgShell` is not `Clone`, so create the popup while holding an
        // immutable borrow of `state`, then release it before the mutable
        // `surfaces.insert` below.
        let popup = {
            let xdg_shell = self
                .state
                .xdg_shell
                .as_ref()
                .expect("xdg_shell presence checked above");
            let positioner = build_positioner(xdg_shell, &config.placement)?;
            let surface = Surface::new(&self.state.compositor_state, &qh)
                .map_err(|e| PresentationError::SurfaceCreate(format!("popup surface: {e}")))?;
            // For a layer parent the role is supplied below via `get_popup`, so
            // the xdg_popup is created with no xdg parent (None) per the
            // wlr-layer-shell contract. A window parent is named here instead.
            let xdg_parent = match &parent {
                PopupParent::Layer(_) => None,
                PopupParent::Window(xdg_surface) => Some(xdg_surface),
            };
            let popup = Popup::from_surface(xdg_parent, &positioner, &qh, surface, xdg_shell)
                .map_err(|e| PresentationError::SurfaceCreate(format!("xdg_popup: {e}")))?;
            if let PopupParent::Layer(layer) = &parent {
                layer.get_popup(popup.xdg_popup());
            }

            // A grab is only valid in response to a recent input serial. Hover-open
            // popovers pass `grab = false` and rely on the core hover-bridge.
            if config.grab {
                let seat = self
                    .state
                    .activation_seat
                    .as_ref()
                    .and_then(|seat_id| self.state.input_seats.get(seat_id))
                    .map(|seat| seat.seat.clone());
                if let (Some(seat), Some(serial)) = (seat.as_ref(), config.grab_serial) {
                    popup.xdg_popup().grab(seat, serial);
                } else {
                    tracing::debug!(
                        "[popover] layer_shell: grab requested for {surface_id} but no seat/serial; opening without grab"
                    );
                }
            }

            // Initial commit (no buffer) maps the popup role and prompts the
            // compositor's first configure.
            popup.wl_surface().commit();
            popup
        };

        let cfg = SurfaceConfig {
            width: config.placement.size.0,
            height: config.placement.size.1,
            // `placement.size` is the *padded* buffer: the popover content plus
            // the ring reserved for descendant shadow/filter overshoot. Carry
            // that reserve so the entry confines input to the visible content.
            padding: config.padding,
            ..SurfaceConfig::default()
        };
        let entry = SurfaceEntry::new(
            WaylandRole::Popup(PopupRole {
                popup,
                parent_id: config.parent_surface_id.clone(),
            }),
            cfg,
            KeyboardMode::None,
        );
        let wl_surface = entry.wl_surface().clone();
        self.state.insert_surface(surface_id.to_string(), entry);

        // Bind HiDPI protocols for the popup surface, mirroring the layer path.
        if let Some(fs) = self
            .state
            .bind_fractional_scale(&wl_surface, &qh, surface_id.to_string())
            && let Some(entry) = self.state.surfaces.get_mut(surface_id)
        {
            entry.fractional_scale = Some(fs);
        }
        if let Some(ref viewporter) = self.state.viewporter {
            let viewport = viewporter.get_viewport(&wl_surface, &qh, ());
            if let Some(entry) = self.state.surfaces.get_mut(surface_id) {
                entry.viewport = Some(viewport);
            }
        }

        Ok(())
    }

    fn reposition_popup(&mut self, surface_id: &str, placement: &PopupPlacement) {
        let Some(xdg_shell) = self.state.xdg_shell.as_ref() else {
            return;
        };
        let Ok(positioner) = build_positioner(xdg_shell, placement) else {
            return;
        };
        if let Some(entry) = self.state.surfaces.get(surface_id)
            && let WaylandRole::Popup(role) = &entry.role
        {
            // `xdg_popup.reposition` requires xdg_wm_base v3+. The token is
            // echoed back on the resulting reactive configure; 0 is fine since
            // we don't correlate repositions yet.
            role.popup.reposition(&positioner, 0);
        }
    }

    /// Tear down a promoted popup. Dropping the [`SurfaceEntry`] drops the SCTK
    /// `Popup`, whose `Drop` destroys the `xdg_popup`/`xdg_surface`/`wl_surface`;
    /// the per-surface viewport/scale/blur objects are released explicitly since
    /// popups are created and destroyed repeatedly.
    pub fn destroy_popup(&mut self, surface_id: &str) {
        let is_popup = self
            .state
            .surfaces
            .get(surface_id)
            .map(|entry| entry.role.is_popup())
            .unwrap_or(false);
        if !is_popup {
            return;
        }
        self.state.teardown_surface(surface_id);
    }

    /// Tear down a top-level layer surface and any popup children it owns.
    pub fn destroy_surface(&mut self, surface_id: &str) {
        // Popups own a separate teardown path (`destroy_popup`) with its own
        // dismissal bookkeeping; parent surfaces — layer surfaces and windows —
        // are torn down here.
        let is_parent_surface = self
            .state
            .surfaces
            .get(surface_id)
            .is_some_and(|entry| !entry.role.is_popup());
        if !is_parent_surface {
            return;
        }
        self.state.teardown_surface_tree(surface_id);
    }

    /// Destroy every popup parented to `parent_surface_id`. The compositor
    /// auto-dismisses popups when their parent surface is destroyed or hidden;
    /// this keeps the backend's own bookkeeping in step (e.g. when the shell
    /// hides the host bar) so stale popup entries are not presented or routed to.
    pub fn destroy_popups_for_parent(&mut self, parent_surface_id: &str) {
        let children: Vec<String> = self
            .state
            .surfaces
            .iter()
            .filter_map(|(id, entry)| match &entry.role {
                WaylandRole::Popup(role) if role.parent_id == parent_surface_id => Some(id.clone()),
                _ => None,
            })
            .collect();
        for id in children {
            self.state.teardown_surface(&id);
        }
    }

    pub fn take_surface_lifecycle_events(&mut self) -> Vec<SurfaceLifecycleEvent> {
        self.state.take_surface_lifecycle_events()
    }

    /// Drain the ids of popups the compositor dismissed since the last call so
    /// the shell can drop the matching popup targets from its own bookkeeping.
    pub fn take_dismissed_popups(&mut self) -> Vec<String> {
        self.state.take_dismissed_popups()
    }

    fn clamp_surface_config(&self, surface_id: &str, cfg: SurfaceConfig) -> SurfaceConfig {
        clamp_surface_config_to_output(cfg, self.output_logical_size_for_surface(surface_id))
    }

    /// Logical size of the output a specific surface is actually displayed
    /// on. Layer surfaces are created with `output: None` (the compositor
    /// picks one), so `entry.output` — populated from `wl_surface::enter` —
    /// is the only reliable way to know which output that was.
    ///
    /// Deliberately returns `None`, not a guess, when `enter` hasn't fired
    /// yet: on multi-monitor setups `wl_surface::enter` for a freshly-mapped
    /// layer surface arrives *after* the compositor's own `configure` (and
    /// so after the resulting re-configure this module sends back — verified
    /// by tracing both events' timestamps), so at clamp time there is no
    /// window where "first output in the registry" is a safe stand-in for
    /// "this surface's output": on a laptop+external setup that guess is
    /// often the *wrong*, smaller monitor, and clamping a compositor-verified
    /// size down to it produces a centered, too-narrow bar with dead space on
    /// both sides. Every value clamped here already came from this surface's
    /// own compositor `configure` ack, so skipping the clamp when the real
    /// output isn't known yet is safe — there is nothing to protect against.
    pub(super) fn output_logical_size_for_surface(&self, surface_id: &str) -> Option<(u32, u32)> {
        let output = self
            .state
            .surfaces
            .get(surface_id)
            .and_then(|entry| entry.output.as_ref())?;
        Self::logical_size_of(&self.state.output_state, output)
    }

    fn logical_size_of(
        output_state: &OutputState,
        output: &wl_output::WlOutput,
    ) -> Option<(u32, u32)> {
        output_state.info(output).and_then(|info| {
            info.logical_size
                .or_else(|| {
                    info.modes
                        .iter()
                        .find(|mode| mode.current)
                        .map(|mode| (mode.dimensions.0, mode.dimensions.1))
                })
                .and_then(|(width, height)| {
                    let width = u32::try_from(width).ok()?;
                    let height = u32::try_from(height).ok()?;
                    Some((width, height))
                })
        })
    }
}
