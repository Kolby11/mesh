use serde::{Deserialize, Serialize};
use std::future::Future;

/// A network connection (wifi, ethernet, vpn, etc.).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkConnection {
    pub id: String,
    pub name: String,
    pub connection_type: ConnectionType,
    pub state: ConnectionState,
    pub signal_strength: Option<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConnectionType {
    Wifi,
    Ethernet,
    Vpn,
    Cellular,
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConnectionState {
    Connected,
    Connecting,
    Disconnected,
    Unavailable,
}

/// A network device (wifi adapter, ethernet port).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkDevice {
    pub id: String,
    pub name: String,
    pub device_type: ConnectionType,
    pub enabled: bool,
}

/// Events emitted by the network backend.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum NetworkEvent {
    ConnectionAdded(NetworkConnection),
    ConnectionRemoved(String),
    ConnectionChanged(NetworkConnection),
    DeviceAdded(NetworkDevice),
    DeviceRemoved(String),
    WifiEnabled(bool),
}

/// The network service trait.
///
/// Backends implement this for specific network managers (NetworkManager, iwd, connman).
pub trait NetworkService: Send + Sync {
    fn backend_id(&self) -> &str;

    fn connections(
        &self,
    ) -> impl Future<Output = Result<Vec<NetworkConnection>, NetworkError>> + Send;
    fn devices(&self) -> impl Future<Output = Result<Vec<NetworkDevice>, NetworkError>> + Send;
    fn active_connection(
        &self,
    ) -> impl Future<Output = Result<Option<NetworkConnection>, NetworkError>> + Send;

    fn connect(&self, connection_id: &str)
    -> impl Future<Output = Result<(), NetworkError>> + Send;
    fn disconnect(
        &self,
        connection_id: &str,
    ) -> impl Future<Output = Result<(), NetworkError>> + Send;

    fn wifi_scan(
        &self,
    ) -> impl Future<Output = Result<Vec<NetworkConnection>, NetworkError>> + Send;
    fn set_wifi_enabled(
        &self,
        enabled: bool,
    ) -> impl Future<Output = Result<(), NetworkError>> + Send;

    fn subscribe(
        &self,
    ) -> impl Future<Output = Result<tokio::sync::broadcast::Receiver<NetworkEvent>, NetworkError>> + Send;
}

#[derive(Debug, thiserror::Error)]
pub enum NetworkError {
    #[error("connection not found: {0}")]
    ConnectionNotFound(String),

    #[error("backend unavailable: {0}")]
    Unavailable(String),

    #[error("{0}")]
    Other(String),
}
