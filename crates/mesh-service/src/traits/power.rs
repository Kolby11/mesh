use serde::{Deserialize, Serialize};
use std::future::Future;

/// Battery information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatteryInfo {
    pub present: bool,
    pub level: f64,
    pub charging: bool,
    pub time_to_empty: Option<u64>,
    pub time_to_full: Option<u64>,
}

/// System power profile.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PowerProfile {
    Performance,
    Balanced,
    PowerSaver,
}

/// Events emitted by the power backend.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PowerEvent {
    BatteryChanged(BatteryInfo),
    ProfileChanged(PowerProfile),
    LidClosed,
    LidOpened,
}

/// The power service trait.
///
/// Backends implement this for specific power management systems (UPower, sysfs, etc.).
pub trait PowerService: Send + Sync {
    fn backend_id(&self) -> &str;

    fn battery(&self) -> impl Future<Output = Result<BatteryInfo, PowerError>> + Send;
    fn profile(&self) -> impl Future<Output = Result<PowerProfile, PowerError>> + Send;
    fn set_profile(&self, profile: PowerProfile) -> impl Future<Output = Result<(), PowerError>> + Send;

    fn subscribe(&self) -> impl Future<Output = Result<tokio::sync::broadcast::Receiver<PowerEvent>, PowerError>> + Send;
}

#[derive(Debug, thiserror::Error)]
pub enum PowerError {
    #[error("no battery present")]
    NoBattery,

    #[error("backend unavailable: {0}")]
    Unavailable(String),

    #[error("{0}")]
    Other(String),
}
