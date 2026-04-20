use serde::{Deserialize, Serialize};
use std::future::Future;

/// A notification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Notification {
    pub id: u32,
    pub app_name: String,
    pub summary: String,
    pub body: Option<String>,
    pub icon: Option<String>,
    pub urgency: Urgency,
    pub timestamp: u64,
    pub actions: Vec<NotificationAction>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Urgency {
    Low,
    Normal,
    Critical,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotificationAction {
    pub id: String,
    pub label: String,
}

/// Events emitted by the notification backend.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum NotificationEvent {
    Posted(Notification),
    Closed(u32),
    ActionInvoked {
        notification_id: u32,
        action_id: String,
    },
}

/// The notification service trait.
///
/// Backends implement this to provide notification listening and management.
pub trait NotificationService: Send + Sync {
    fn backend_id(&self) -> &str;

    fn list(&self) -> impl Future<Output = Result<Vec<Notification>, NotificationError>> + Send;
    fn close(&self, id: u32) -> impl Future<Output = Result<(), NotificationError>> + Send;
    fn close_all(&self) -> impl Future<Output = Result<(), NotificationError>> + Send;
    fn invoke_action(
        &self,
        notification_id: u32,
        action_id: &str,
    ) -> impl Future<Output = Result<(), NotificationError>> + Send;

    fn subscribe(
        &self,
    ) -> impl Future<
        Output = Result<tokio::sync::broadcast::Receiver<NotificationEvent>, NotificationError>,
    > + Send;
}

#[derive(Debug, thiserror::Error)]
pub enum NotificationError {
    #[error("notification not found: {0}")]
    NotFound(u32),

    #[error("backend unavailable: {0}")]
    Unavailable(String),

    #[error("{0}")]
    Other(String),
}
