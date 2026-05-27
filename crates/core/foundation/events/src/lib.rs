/// Typed event bus and inter-module communication for MESH.
use serde_json::Value;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use tokio::sync::broadcast;

/// An event published on the bus.
#[derive(Debug, Clone)]
pub struct Event {
    pub channel: String,
    pub source: String,
    pub payload: Value,
}

/// Handle for subscribing to and publishing events.
///
/// The internal channel map is guarded by an `RwLock` so publishes on
/// already-known channels take a shared lock and never serialize against
/// each other. Subscription and first-time publication still take the
/// exclusive lock to register the channel.
#[derive(Debug, Clone)]
pub struct EventBus {
    inner: Arc<RwLock<EventBusInner>>,
}

#[derive(Debug)]
struct EventBusInner {
    channels: HashMap<String, broadcast::Sender<Event>>,
}

impl EventBus {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(RwLock::new(EventBusInner {
                channels: HashMap::new(),
            })),
        }
    }

    /// Subscribe to a named channel. Returns a receiver for incoming events.
    pub fn subscribe(&self, channel: &str) -> broadcast::Receiver<Event> {
        // Fast path: channel already exists, only take the read lock.
        if let Ok(inner) = self.inner.read() {
            if let Some(sender) = inner.channels.get(channel) {
                return sender.subscribe();
            }
        }
        // Slow path: install the channel and subscribe.
        let mut inner = self.inner.write().unwrap();
        let sender = inner
            .channels
            .entry(channel.to_string())
            .or_insert_with(|| broadcast::channel(256).0);
        sender.subscribe()
    }

    /// Publish an event to a named channel.
    pub fn publish(&self, event: Event) -> Result<(), EventError> {
        let inner = self.inner.read().unwrap();
        if let Some(sender) = inner.channels.get(&event.channel) {
            let _ = sender.send(event);
        }
        Ok(())
    }

    /// List all active channels.
    pub fn channels(&self) -> Vec<String> {
        let inner = self.inner.read().unwrap();
        inner.channels.keys().cloned().collect()
    }
}

impl Default for EventBus {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, thiserror::Error)]
pub enum EventError {
    #[error("channel not found: {0}")]
    ChannelNotFound(String),

    #[error("invalid payload: {0}")]
    InvalidPayload(String),
}
