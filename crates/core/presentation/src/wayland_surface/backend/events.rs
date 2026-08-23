use super::*;

impl WaylandSurfaceBackend {
    pub fn pump(&mut self) -> Result<(), PresentationError> {
        self.dispatch_available()?;
        self.release_expired_surface_focus_grab()
    }

    /// Flush all requests staged by this shell frame, then dispatch already
    /// available events once. Per-surface presentation intentionally does not
    /// progress the shared Wayland connection so independent surfaces can be
    /// prepared and painted before one outbound flush.
    pub fn finish_frame(&mut self) -> Result<(), PresentationError> {
        self.dispatch_pending()
    }

    pub fn poll_events(&mut self) -> Result<Vec<DevWindowEvent>, PresentationError> {
        self.dispatch_available()?;
        self.release_expired_surface_focus_grab()?;
        self.state.push_due_keyboard_repeats();
        let events = std::mem::take(&mut self.state.events)
            .into_iter()
            .map(|queued| queued.event)
            .collect::<Vec<_>>();
        if !events.is_empty() {
            tracing::trace!(
                "[hover] layer_shell: draining {} input event(s)",
                events.len()
            );
        }
        Ok(events)
    }

    fn release_expired_surface_focus_grab(&mut self) -> Result<(), PresentationError> {
        if self.state.release_expired_surface_focus_grab() {
            match self.event_queue.flush() {
                Ok(()) => {}
                Err(error) => return Err(self.map_wayland_error("flush", error)),
            }
        }
        Ok(())
    }

    fn dispatch_pending(&mut self) -> Result<(), PresentationError> {
        match self.event_queue.flush() {
            Ok(()) => {}
            Err(error) => return Err(self.map_wayland_error("flush", error)),
        }
        match self.event_queue.dispatch_pending(&mut self.state) {
            Ok(_) => {}
            Err(error) => return Err(self.map_dispatch_error("dispatch", error)),
        }
        self.release_expired_surface_focus_grab()?;
        Ok(())
    }

    pub(super) fn dispatch_available(&mut self) -> Result<(), PresentationError> {
        match self.event_queue.flush() {
            Ok(()) => {}
            Err(error) => return Err(self.map_wayland_error("flush", error)),
        }

        for _ in 0..32 {
            match self.event_queue.dispatch_pending(&mut self.state) {
                Ok(_) => {}
                Err(error) => return Err(self.map_dispatch_error("dispatch", error)),
            }

            let Some(read_guard) = self.event_queue.prepare_read() else {
                continue;
            };

            let poll_result = {
                let fd = read_guard.connection_fd();
                let mut fds = [PollFd::new(
                    &fd,
                    PollFlags::IN | PollFlags::ERR | PollFlags::HUP,
                )];
                poll(&mut fds, 0).map(|ready| {
                    if ready == 0 {
                        None
                    } else {
                        Some(fds[0].revents())
                    }
                })
            };

            match poll_result {
                Ok(None) => {
                    drop(read_guard);
                    break;
                }
                Ok(Some(revents)) => {
                    if !revents.intersects(PollFlags::IN | PollFlags::ERR | PollFlags::HUP) {
                        drop(read_guard);
                        break;
                    }

                    match read_guard.read() {
                        Ok(read_count) => {
                            tracing::trace!("read {read_count} Wayland event(s)");
                            if read_count == 0 {
                                return Err(self.connection_lost("read: Wayland connection closed"));
                            }
                        }
                        Err(WaylandError::Io(err)) if err.kind() == ErrorKind::WouldBlock => break,
                        Err(err) => {
                            return Err(self.map_wayland_error("read", err));
                        }
                    }
                }
                Err(rustix::io::Errno::INTR) => {
                    drop(read_guard);
                    break;
                }
                Err(err) => {
                    drop(read_guard);
                    return Err(self.connection_lost(format!("poll: {err}")));
                }
            }
        }

        match self.event_queue.dispatch_pending(&mut self.state) {
            Ok(_) => {}
            Err(error) => return Err(self.map_dispatch_error("dispatch", error)),
        }
        self.release_expired_surface_focus_grab()?;
        Ok(())
    }

    /// Block on the Wayland connection fd until `timeout` elapses or a wakeup occurs.
    ///
    /// After the Wayland poll returns (or times out), checks `eventfd_fd` with a
    /// 0ms poll to detect IPC/backend signals. Reads and consumes the eventfd
    /// counter when signaled.
    pub fn wait_for_events(
        &mut self,
        timeout: std::time::Duration,
        eventfd_fd: std::os::unix::io::BorrowedFd<'_>,
    ) -> Result<crate::WaitResult, crate::PresentationError> {
        use crate::{WaitReason, WaitResult};
        use rustix::io::read as eventfd_read;

        match self.event_queue.flush() {
            Ok(()) => {}
            Err(error) => return Err(self.map_wayland_error("flush", error)),
        }
        match self.event_queue.dispatch_pending(&mut self.state) {
            Ok(_) => {}
            Err(error) => return Err(self.map_dispatch_error("dispatch", error)),
        }

        // A `None` guard means events arrived between the dispatch above and
        // here; don't block, let the caller process them.
        let Some(read_guard) = self.event_queue.prepare_read() else {
            return Ok(WaitResult {
                reason: WaitReason::WaylandEvent,
            });
        };

        // Block on both Wayland and the shell eventfd. Backend/IPC work
        // must be able to interrupt long idle waits once the shell is no
        // longer clamped to a fixed 16ms loop cadence.
        let wayland_fd = read_guard.connection_fd();
        let mut fds = [
            PollFd::new(&wayland_fd, PollFlags::IN | PollFlags::ERR | PollFlags::HUP),
            PollFd::new(&eventfd_fd, PollFlags::IN | PollFlags::ERR | PollFlags::HUP),
        ];
        let timeout_ms = timeout.as_millis().min(i32::MAX as u128) as i32;

        let (wayland_ready, ipc_ready) = match poll(&mut fds, timeout_ms) {
            Ok(0) => {
                drop(read_guard);
                return Ok(WaitResult::deadline_expired());
            }
            Err(rustix::io::Errno::INTR) => (false, false),
            Ok(_) => (
                fds[0]
                    .revents()
                    .intersects(PollFlags::IN | PollFlags::ERR | PollFlags::HUP),
                fds[1]
                    .revents()
                    .intersects(PollFlags::IN | PollFlags::ERR | PollFlags::HUP),
            ),
            Err(err) => {
                drop(read_guard);
                return Err(self.connection_lost(format!("poll: {err}")));
            }
        };

        // Consume the eventfd counter so it doesn't keep firing.
        if ipc_ready {
            let mut counter = [0u8; 8];
            let _ = eventfd_read(&eventfd_fd, &mut counter);
        }

        let mut wake_reason = WaitReason::DeadlineExpired;
        if wayland_ready {
            match read_guard.read() {
                Ok(0) => {
                    return Err(self.connection_lost("read: Wayland connection closed"));
                }
                Ok(_) => {
                    wake_reason = WaitReason::WaylandEvent;
                }
                Err(WaylandError::Io(err)) if err.kind() == ErrorKind::WouldBlock => {
                    wake_reason = WaitReason::WaylandEvent;
                }
                Err(err) => {
                    return Err(self.map_wayland_error("read", err));
                }
            }
        } else {
            drop(read_guard);
        }

        // If eventfd fired, that takes priority as the reported reason.
        if ipc_ready {
            wake_reason = WaitReason::IpcEvent;
        }

        match self.event_queue.dispatch_pending(&mut self.state) {
            Ok(_) => {}
            Err(error) => return Err(self.map_dispatch_error("dispatch", error)),
        }
        self.release_expired_surface_focus_grab()?;

        Ok(WaitResult {
            reason: wake_reason,
        })
    }

    fn connection_lost(&mut self, reason: impl Into<String>) -> PresentationError {
        let reason = reason.into();
        self.state.mark_connection_lost(reason.clone());
        PresentationError::ConnectionLost(reason)
    }

    fn map_wayland_error(&mut self, context: &str, error: WaylandError) -> PresentationError {
        let message = format!("{context}: {error}");
        match error {
            WaylandError::Io(error) if error.kind() != ErrorKind::WouldBlock => {
                self.connection_lost(message)
            }
            _ => PresentationError::SurfaceCreate(message),
        }
    }

    fn map_dispatch_error(&mut self, context: &str, error: DispatchError) -> PresentationError {
        match error {
            DispatchError::Backend(error) => self.map_wayland_error(context, error),
            error => PresentationError::SurfaceCreate(format!("{context}: {error}")),
        }
    }
}
