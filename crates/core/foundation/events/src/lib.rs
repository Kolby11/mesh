/// Typed event bus and inter-plugin communication for MESH.
use serde_json::Value;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tokio::sync::broadcast;

/// An event published on the bus.
#[derive(Debug, Clone)]
pub struct Event {
    pub channel: String,
    pub source: String,
    pub payload: Value,
}

/// Handle for subscribing to and publishing events.
#[derive(Debug, Clone)]
pub struct EventBus {
    inner: Arc<Mutex<EventBusInner>>,
}

#[derive(Debug)]
struct EventBusInner {
    channels: HashMap<String, broadcast::Sender<Event>>,
}

impl EventBus {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(EventBusInner {
                channels: HashMap::new(),
            })),
        }
    }

    /// Subscribe to a named channel. Returns a receiver for incoming events.
    pub fn subscribe(&self, channel: &str) -> broadcast::Receiver<Event> {
        let mut inner = self.inner.lock().unwrap();
        let sender = inner
            .channels
            .entry(channel.to_string())
            .or_insert_with(|| broadcast::channel(256).0);
        sender.subscribe()
    }

    /// Publish an event to a named channel.
    pub fn publish(&self, event: Event) -> Result<(), EventError> {
        let inner = self.inner.lock().unwrap();
        if let Some(sender) = inner.channels.get(&event.channel) {
            let _ = sender.send(event);
        }
        Ok(())
    }

    /// List all active channels.
    pub fn channels(&self) -> Vec<String> {
        let inner = self.inner.lock().unwrap();
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
