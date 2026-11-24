use crate::{ResourceByteBudget, ResourceByteReservation};
use std::collections::HashMap;
use std::fmt::Write;
use std::sync::{Arc, Mutex, MutexGuard, OnceLock, Weak};

pub const FRAME_SCHEME: &str = "mesh-frame:";
const HANDLE_HEX_LEN: usize = 32;
const MAX_FRAME_DIMENSION: u32 = 4096;
const MAX_FRAME_BYTES: usize = 8 * 1024 * 1024;
const MAX_OWNER_BYTES: usize = 32 * 1024 * 1024;
const MAX_FRAME_BYTES_TOTAL: usize = 128 * 1024 * 1024;
const MAX_OWNER_CHANNELS: usize = 8;

/// Whether the value has the syntax of a live frame handle.
pub fn is_frame_handle(value: &str) -> bool {
    value.strip_prefix(FRAME_SCHEME).is_some_and(|id| {
        id.len() == HANDLE_HEX_LEN
            && id
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    })
}

/// Notification for one frame replacement or stream retirement.
pub trait FrameObserver: Send + Sync {
    fn frame_changed(&self);
}

/// An immutable snapshot of tightly packed straight-alpha RGBA8 pixels.
pub struct LiveFrame {
    width: u32,
    height: u32,
    rgba: Vec<u8>,
    _owner_budget: ResourceByteReservation,
    _total_budget: ResourceByteReservation,
}

impl LiveFrame {
    pub fn size(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    pub fn rgba(&self) -> &[u8] {
        &self.rgba
    }
}

struct FrameSlot {
    latest: Arc<LiveFrame>,
    observers: Vec<Weak<dyn FrameObserver>>,
}

type FrameRegistry = HashMap<Arc<str>, FrameSlot>;

fn slots() -> MutexGuard<'static, FrameRegistry> {
    static SLOTS: OnceLock<Mutex<FrameRegistry>> = OnceLock::new();
    SLOTS
        .get_or_init(Mutex::default)
        .lock()
        .unwrap_or_else(|error| error.into_inner())
}

/// Read the current frame and subscribe to the next change in one operation.
/// The subscription is weak and is consumed when the stream changes.
pub fn latest_frame(
    handle: &str,
    observer: Option<Weak<dyn FrameObserver>>,
) -> Option<Arc<LiveFrame>> {
    let mut slots = slots();
    let slot = slots.get_mut(handle)?;
    if let Some(observer) = observer {
        slot.observers.retain(|entry| entry.strong_count() > 0);
        if !slot.observers.iter().any(|entry| entry.ptr_eq(&observer)) {
            slot.observers.push(observer);
        }
    }
    Some(Arc::clone(&slot.latest))
}

#[derive(Debug, thiserror::Error)]
pub enum FrameError {
    #[error(
        "frame channel names must contain 1 to 64 ASCII letters, digits, dashes, or underscores"
    )]
    InvalidName,
    #[error("frame dimensions {0}x{1} must fit within 1..={MAX_FRAME_DIMENSION}")]
    InvalidSize(u32, u32),
    #[error("frame needs exactly {expected} RGBA8 bytes but has {actual}")]
    InvalidLength { expected: usize, actual: usize },
    #[error("frame exceeds the {MAX_FRAME_BYTES}-byte limit")]
    TooLarge,
    #[error("provider already owns {MAX_OWNER_CHANNELS} frame channels")]
    ChannelLimit,
    #[error("live frame storage budget exceeded")]
    Budget,
    #[error("could not allocate an opaque live frame handle")]
    Entropy,
    #[error("state references live frame '{0}', which this provider does not own")]
    Unregistered(String),
}

/// One producer generation's registered frame streams.
#[derive(Debug)]
pub struct FrameOwner {
    channels: Mutex<HashMap<String, Arc<str>>>,
    budget: ResourceByteBudget,
}

impl Default for FrameOwner {
    fn default() -> Self {
        Self {
            channels: Mutex::default(),
            budget: ResourceByteBudget::new(MAX_OWNER_BYTES),
        }
    }
}

impl FrameOwner {
    /// Publish one frame, replacing the previous frame of the named channel.
    /// The first publication creates its handle. A rejected frame leaves the
    /// previous contents and handle available.
    pub fn publish(
        &self,
        name: &str,
        width: u32,
        height: u32,
        rgba: Vec<u8>,
    ) -> Result<Arc<str>, FrameError> {
        static TOTAL: OnceLock<ResourceByteBudget> = OnceLock::new();
        if name.is_empty()
            || name.len() > 64
            || !name
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || b"-_".contains(&byte))
        {
            return Err(FrameError::InvalidName);
        }
        if width == 0 || height == 0 || width > MAX_FRAME_DIMENSION || height > MAX_FRAME_DIMENSION
        {
            return Err(FrameError::InvalidSize(width, height));
        }
        let expected = width as usize * height as usize * 4;
        if expected > MAX_FRAME_BYTES {
            return Err(FrameError::TooLarge);
        }
        if rgba.len() != expected {
            return Err(FrameError::InvalidLength {
                expected,
                actual: rgba.len(),
            });
        }
        let (handle, observers) = {
            let mut channels = self
                .channels
                .lock()
                .unwrap_or_else(|error| error.into_inner());
            let existing = channels.get(name).cloned();
            if existing.is_none() && channels.len() >= MAX_OWNER_CHANNELS {
                return Err(FrameError::ChannelLimit);
            }
            let latest = Arc::new(LiveFrame {
                width,
                height,
                rgba,
                _owner_budget: self
                    .budget
                    .try_reserve(expected)
                    .ok_or(FrameError::Budget)?,
                _total_budget: TOTAL
                    .get_or_init(|| ResourceByteBudget::new(MAX_FRAME_BYTES_TOTAL))
                    .try_reserve(expected)
                    .ok_or(FrameError::Budget)?,
            });
            let mut slots = slots();
            if let Some(handle) = existing {
                let slot = slots
                    .get_mut(&handle)
                    .expect("channel owns a registered slot");
                slot.latest = latest;
                (handle, std::mem::take(&mut slot.observers))
            } else {
                let handle = new_handle()?;
                if slots.contains_key(&handle) {
                    return Err(FrameError::Entropy);
                }
                slots.insert(
                    Arc::clone(&handle),
                    FrameSlot {
                        latest,
                        observers: Vec::new(),
                    },
                );
                channels.insert(name.to_string(), Arc::clone(&handle));
                (handle, Vec::new())
            }
        };
        notify(observers);
        Ok(handle)
    }

    /// Verify that every referenced live stream is owned by this producer.
    pub fn validate<'a>(
        &self,
        handles: impl IntoIterator<Item = &'a str>,
    ) -> Result<(), FrameError> {
        let channels = self
            .channels
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        for handle in handles {
            if !channels
                .values()
                .any(|registered| registered.as_ref() == handle)
            {
                return Err(FrameError::Unregistered(handle.into()));
            }
        }
        Ok(())
    }

    /// Retire a channel. The handle stops resolving immediately.
    pub fn close(&self, name: &str) -> bool {
        let slot = {
            let mut channels = self
                .channels
                .lock()
                .unwrap_or_else(|error| error.into_inner());
            channels
                .remove(name)
                .and_then(|handle| slots().remove(&handle))
        };
        if let Some(slot) = slot {
            notify(slot.observers);
            true
        } else {
            false
        }
    }

    /// Retire all channels when the producer stops.
    pub fn clear(&self) {
        let retired = {
            let mut channels = self
                .channels
                .lock()
                .unwrap_or_else(|error| error.into_inner());
            let mut slots = slots();
            channels
                .drain()
                .filter_map(|(_, handle)| slots.remove(&handle))
                .collect::<Vec<_>>()
        };
        for slot in retired {
            notify(slot.observers);
        }
    }
}

impl Drop for FrameOwner {
    fn drop(&mut self) {
        self.clear();
    }
}

fn new_handle() -> Result<Arc<str>, FrameError> {
    let mut bytes = [0_u8; HANDLE_HEX_LEN / 2];
    getrandom::fill(&mut bytes).map_err(|_| FrameError::Entropy)?;
    let mut handle = String::with_capacity(FRAME_SCHEME.len() + HANDLE_HEX_LEN);
    handle.push_str(FRAME_SCHEME);
    for byte in bytes {
        write!(handle, "{byte:02x}").expect("formatting into a String cannot fail");
    }
    Ok(handle.into())
}

fn notify(observers: Vec<Weak<dyn FrameObserver>>) {
    for observer in observers
        .into_iter()
        .filter_map(|observer| observer.upgrade())
    {
        observer.frame_changed();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalid_live_frame_publications_leave_the_latest_frame_intact() {
        let owner = FrameOwner::default();
        let pixels = vec![1, 2, 3, 255];
        let handle = owner.publish("test", 1, 1, pixels.clone()).unwrap();
        assert!(is_frame_handle(&handle));
        assert!(matches!(
            owner.publish("test", 0, 1, Vec::new()),
            Err(FrameError::InvalidSize(..))
        ));
        assert!(matches!(
            owner.publish("test", 1, 1, vec![0, 1, 2]),
            Err(FrameError::InvalidLength { .. })
        ));
        assert!(matches!(
            owner.publish("test", 4096, 4096, Vec::new()),
            Err(FrameError::TooLarge)
        ));
        assert_eq!(latest_frame(&handle, None).unwrap().rgba(), pixels);

        let other = FrameOwner::default();
        assert!(other.validate([handle.as_ref()]).is_err());
        assert_ne!(other.publish("test", 1, 1, pixels.clone()).unwrap(), handle);
        assert!(owner.close("test"));
        assert_ne!(owner.publish("test", 1, 1, pixels).unwrap(), handle);
        assert!(latest_frame(&handle, None).is_none());
    }

    #[test]
    fn live_frame_owner_bounds_distinct_channels() {
        let owner = FrameOwner::default();
        for index in 0..MAX_OWNER_CHANNELS {
            owner
                .publish(&format!("slot_{index}"), 1, 1, vec![0; 4])
                .unwrap();
        }
        assert!(matches!(
            owner.publish("overflow", 1, 1, vec![0; 4]),
            Err(FrameError::ChannelLimit)
        ));
        assert!(owner.publish("slot_0", 1, 1, vec![255; 4]).is_ok());
        assert!(owner.close("slot_0"));
        assert!(owner.publish("overflow", 1, 1, vec![0; 4]).is_ok());
    }
}
