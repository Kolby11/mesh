use std::sync::{Arc, Mutex, Weak};

use tokio::sync::{OwnedSemaphorePermit, Semaphore, mpsc};

use crate::shell::{ShellMessage, WakeHandle};

#[derive(Debug)]
pub(in crate::shell) struct BackendIngressEntry {
    message: Mutex<Option<ShellMessage>>,
    _permit: OwnedSemaphorePermit,
}

impl BackendIngressEntry {
    pub(in crate::shell) fn take(&self) -> ShellMessage {
        self.message
            .lock()
            .unwrap()
            .take()
            .expect("one shell consumer")
    }
}

#[derive(Clone)]
pub(in crate::shell) struct BackendIngress {
    capacity: Arc<Semaphore>,
    tail: Arc<Mutex<Weak<BackendIngressEntry>>>,
}

impl Default for BackendIngress {
    fn default() -> Self {
        Self {
            capacity: Arc::new(Semaphore::new(
                mesh_core_backend::BACKEND_EVENT_QUEUE_CAPACITY,
            )),
            tail: Arc::default(),
        }
    }
}

impl BackendIngress {
    pub(in crate::shell) fn close(&self) {
        self.capacity.close();
    }

    pub(super) async fn send(
        &self,
        tx: &mpsc::UnboundedSender<ShellMessage>,
        wake: &WakeHandle,
        message: ShellMessage,
    ) -> bool {
        if self.capacity.is_closed() || tx.is_closed() {
            return false;
        }
        let mut message = Some(message);
        if Self::replace(&self.tail.lock().unwrap(), &mut message) {
            return true;
        }
        let permit = tokio::select! {
            result = self.capacity.clone().acquire_owned() => {
                let Ok(permit) = result else { return false };
                permit
            }
            _ = tx.closed() => return false,
        };
        let mut tail = self.tail.lock().unwrap();
        if Self::replace(&tail, &mut message) {
            return true;
        }
        let message = message.expect("unconsumed backend message");
        let snapshot = matches!(message, ShellMessage::BackendServiceUpdate { .. });
        let entry = Arc::new(BackendIngressEntry {
            message: Mutex::new(Some(message)),
            _permit: permit,
        });
        *tail = if snapshot {
            Arc::downgrade(&entry)
        } else {
            Weak::new()
        };
        if tx.send(ShellMessage::BackendIngress(entry)).is_err() {
            return false;
        }
        wake.wake();
        true
    }

    fn replace(tail: &Weak<BackendIngressEntry>, message: &mut Option<ShellMessage>) -> bool {
        let Some(ShellMessage::BackendServiceUpdate {
            interface,
            provider_id,
            identity,
            ..
        }) = message.as_ref()
        else {
            return false;
        };
        let Some(entry) = tail.upgrade() else {
            return false;
        };
        let mut current = entry.message.lock().unwrap();
        if matches!(
            current.as_ref(),
            Some(ShellMessage::BackendServiceUpdate {
                interface: previous_interface,
                provider_id: previous_provider,
                identity: previous_identity,
                ..
            }) if interface == previous_interface && provider_id == previous_provider
                && identity == previous_identity
        ) {
            *current = message.take();
            return true;
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shell::ServiceEvent;

    fn update(value: usize) -> ShellMessage {
        ShellMessage::BackendServiceUpdate {
            interface: "mesh.test".into(),
            provider_id: "@test/provider".into(),
            identity: Default::default(),
            event: ServiceEvent::Updated {
                service: "mesh.test".into(),
                source_module: "@test/provider".into(),
                payload: serde_json::json!({"value": value}),
            },
        }
    }

    fn result(call_id: u64) -> ShellMessage {
        ShellMessage::BackendCommandResult {
            interface: "mesh.test".into(),
            provider_id: "@test/provider".into(),
            identity: Default::default(),
            generation: 0,
            call_id: mesh_core_backend::CallId::from_raw(call_id),
            command: "test".into(),
            result: serde_json::Value::Null,
            outcome: mesh_core_backend::BackendCommandOutcome::Completed,
        }
    }

    fn dequeue(rx: &mut mpsc::UnboundedReceiver<ShellMessage>) -> ShellMessage {
        let ShellMessage::BackendIngress(entry) = rx.try_recv().unwrap() else {
            panic!("backend ingress entry");
        };
        entry.take()
    }

    #[tokio::test]
    async fn backend_ingress_coalesces_paused_snapshot_bursts_and_backpressures_results() {
        let ingress = BackendIngress::default();
        let (tx, mut rx) = mpsc::unbounded_channel();
        let fd = rustix::event::eventfd(0, rustix::event::EventfdFlags::NONBLOCK).unwrap();
        let wake = WakeHandle::from_fd(&fd).unwrap();
        for size in [1_000, 10_000, 100_000] {
            let start = std::time::Instant::now();
            for value in 0..size {
                assert!(ingress.send(&tx, &wake, update(value)).await);
            }
            let elapsed = start.elapsed();
            assert_eq!(rx.len(), 1, "paused burst of {size}");
            let ShellMessage::BackendServiceUpdate {
                event: ServiceEvent::Updated { payload, .. },
                ..
            } = dequeue(&mut rx)
            else {
                panic!("service snapshot");
            };
            assert_eq!(payload["value"], size - 1);
            eprintln!("paused ingress burst {size}: 1 queued snapshot in {elapsed:?}");
        }

        assert!(ingress.send(&tx, &wake, update(123)).await);
        assert!(ingress.send(&tx, &wake, result(1)).await);
        assert!(ingress.send(&tx, &wake, update(456)).await);
        for call_id in 2..mesh_core_backend::BACKEND_EVENT_QUEUE_CAPACITY as u64 - 1 {
            assert!(ingress.send(&tx, &wake, result(call_id)).await);
        }
        let capacity = mesh_core_backend::BACKEND_EVENT_QUEUE_CAPACITY;
        assert_eq!(rx.len(), capacity);
        let mut blocked = std::pin::pin!(ingress.send(&tx, &wake, result(capacity as u64)));
        assert!(
            std::future::poll_fn(|cx| {
                std::task::Poll::Ready(std::future::Future::poll(blocked.as_mut(), cx).is_pending())
            })
            .await
        );
        assert!(matches!(
            dequeue(&mut rx),
            ShellMessage::BackendServiceUpdate {
                event: ServiceEvent::Updated { payload, .. }, ..
            } if payload["value"] == 123
        ));
        assert!(blocked.await);
        assert_eq!(rx.len(), capacity);
        assert!(
            matches!(dequeue(&mut rx), ShellMessage::BackendCommandResult { call_id, .. } if call_id.raw() == 1)
        );
        assert!(matches!(
            dequeue(&mut rx),
            ShellMessage::BackendServiceUpdate {
                event: ServiceEvent::Updated { payload, .. }, ..
            } if payload["value"] == 456
        ));
        for expected in (2..capacity as u64 - 1).chain([capacity as u64]) {
            assert!(
                matches!(dequeue(&mut rx), ShellMessage::BackendCommandResult { call_id, .. } if call_id.raw() == expected)
            );
        }
        assert!(rx.is_empty());
    }
}
