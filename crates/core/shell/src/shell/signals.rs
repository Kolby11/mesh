use super::WakeHandle;
use super::types::{CoreRequest, ShellMessage};
use tokio::runtime::Runtime;
use tokio::signal::unix::{SignalKind, signal};
use tokio::sync::mpsc;

const SIGHUP: i32 = 1;
const SIGINT: i32 = 2;
const SIGTERM: i32 = 15;

/// Route SIGINT, SIGTERM and SIGHUP into the shell's ordinary shutdown, so
/// backend teardown reaps stream subprocesses instead of orphaning them.
///
/// Installing a handler replaces the default terminate action for the rest of
/// the process lifetime, so a repeated signal during a stuck teardown exits
/// immediately with the conventional `128 + signo` status.
pub(super) fn spawn_signal_shutdown(
    runtime: &Runtime,
    tx: mpsc::UnboundedSender<ShellMessage>,
    wake: WakeHandle,
) -> std::io::Result<()> {
    let _guard = runtime.enter();
    let mut interrupt = signal(SignalKind::interrupt())?;
    let mut terminate = signal(SignalKind::terminate())?;
    let mut hangup = signal(SignalKind::hangup())?;
    runtime.spawn(async move {
        let mut requested = false;
        loop {
            let signo = tokio::select! {
                Some(()) = interrupt.recv() => SIGINT,
                Some(()) = terminate.recv() => SIGTERM,
                Some(()) = hangup.recv() => SIGHUP,
                else => return,
            };
            if requested {
                tracing::warn!(
                    signal = signo,
                    "repeated signal during shutdown; exiting now"
                );
                std::process::exit(128 + signo);
            }
            requested = true;
            tracing::info!(signal = signo, "shutdown requested by signal");
            if tx.send(ShellMessage::Ipc(CoreRequest::Shutdown)).is_err() {
                std::process::exit(128 + signo);
            }
            wake.wake();
        }
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::fd::AsFd;

    #[test]
    fn signal_requests_ordinary_shutdown_and_wakes_the_loop() {
        let runtime = Runtime::new().unwrap();
        let eventfd = rustix::event::eventfd(0, rustix::event::EventfdFlags::NONBLOCK).unwrap();
        let wake = WakeHandle::from_fd(&eventfd).unwrap();
        let (tx, mut rx) = mpsc::unbounded_channel();
        spawn_signal_shutdown(&runtime, tx, wake).unwrap();

        assert_eq!(unsafe { libc::raise(libc::SIGHUP) }, 0);
        let message = runtime.block_on(async {
            tokio::time::timeout(std::time::Duration::from_secs(2), rx.recv()).await
        });

        assert!(matches!(
            message,
            Ok(Some(ShellMessage::Ipc(CoreRequest::Shutdown)))
        ));
        let mut counter = [0u8; 8];
        rustix::io::read(eventfd.as_fd(), &mut counter).unwrap();
        assert_eq!(u64::from_ne_bytes(counter), 1);
    }
}
