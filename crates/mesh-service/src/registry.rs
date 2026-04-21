/// Legacy typed registry used while Rust-side callers finish migrating to
/// plugin-declared interfaces.
///
/// The long-term runtime path is contract + provider lookup via the interface
/// registry. This registry still holds one active typed backend per service so
/// older Rust integrations can keep working during the transition.
///
/// ```text
/// Backend plugin registers:   registry.register_audio(Box::new(PipewireBackend))
/// Frontend widget looks up:   registry.audio()  ->  &dyn AudioService
/// ```
use std::any::{Any, TypeId};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

/// Metadata about a registered service backend.
#[derive(Debug, Clone)]
pub struct ServiceEntry {
    /// The service type name (e.g. "audio", "network").
    pub service_type: String,
    /// The active backend's identifier (e.g. "pipewire", "networkmanager").
    pub backend_id: String,
    /// The plugin that provides this backend (e.g. "@mesh/pipewire-audio").
    pub plugin_id: String,
}

/// Central registry that holds one active typed backend per service type.
///
/// New frontend plugins should prefer `mesh.interfaces.get(...)` and contract
/// providers. This registry is for compatibility with older Rust call sites.
#[derive(Default)]
pub struct ServiceRegistry {
    services: RwLock<HashMap<TypeId, Arc<dyn Any + Send + Sync>>>,
    entries: RwLock<Vec<ServiceEntry>>,
}

impl ServiceRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a backend for a service type.
    ///
    /// If a backend for this type is already registered, it is replaced.
    ///
    /// # Example
    ///
    /// ```ignore
    /// registry.register::<dyn AudioService>(
    ///     "audio",
    ///     "pipewire",
    ///     "@mesh/pipewire-audio",
    ///     Arc::new(pipewire_backend),
    /// );
    /// ```
    pub fn register<T: ?Sized + 'static>(
        &self,
        service_type: &str,
        backend_id: &str,
        plugin_id: &str,
        backend: Arc<T>,
    ) where
        T: Send + Sync,
    {
        let type_id = TypeId::of::<T>();
        let boxed: Arc<dyn Any + Send + Sync> = Arc::new(backend);

        self.services.write().unwrap().insert(type_id, boxed);

        let mut entries = self.entries.write().unwrap();
        entries.retain(|e| e.service_type != service_type);
        entries.push(ServiceEntry {
            service_type: service_type.to_string(),
            backend_id: backend_id.to_string(),
            plugin_id: plugin_id.to_string(),
        });
    }

    /// Look up the active backend for a service type.
    ///
    /// Returns `None` if no backend is registered for this type.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let audio: Arc<dyn AudioService> = registry.get::<dyn AudioService>().unwrap();
    /// let devices = audio.output_devices().await?;
    /// ```
    pub fn get<T: ?Sized + 'static>(&self) -> Option<Arc<T>>
    where
        T: Send + Sync,
    {
        let type_id = TypeId::of::<T>();
        let services = self.services.read().unwrap();
        let any = services.get(&type_id)?;
        // The stored value is Arc<Arc<T>>, so we downcast to Arc<T>.
        any.downcast_ref::<Arc<T>>().cloned()
    }

    /// List all registered service entries.
    pub fn list(&self) -> Vec<ServiceEntry> {
        self.entries.read().unwrap().clone()
    }

    /// Check if a service type has a registered backend.
    pub fn has<T: ?Sized + 'static>(&self) -> bool
    where
        T: Send + Sync,
    {
        let type_id = TypeId::of::<T>();
        self.services.read().unwrap().contains_key(&type_id)
    }
}

impl std::fmt::Debug for ServiceRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ServiceRegistry")
            .field("entries", &self.entries.read().unwrap())
            .finish()
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ServiceError {
    #[error("no backend registered for service: {0}")]
    NoBackend(String),

    #[error("backend conflict: {existing} already registered for {service}, cannot register {new}")]
    Conflict {
        service: String,
        existing: String,
        new: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    trait DummyService: Send + Sync {
        fn name(&self) -> &str;
    }

    struct DummyBackendA;
    impl DummyService for DummyBackendA {
        fn name(&self) -> &str {
            "backend-a"
        }
    }

    struct DummyBackendB;
    impl DummyService for DummyBackendB {
        fn name(&self) -> &str {
            "backend-b"
        }
    }

    #[test]
    fn register_and_get() {
        let registry = ServiceRegistry::new();
        let backend: Arc<dyn DummyService> = Arc::new(DummyBackendA);

        registry.register::<dyn DummyService>("dummy", "a", "@test/a", backend);

        let retrieved = registry.get::<dyn DummyService>().unwrap();
        assert_eq!(retrieved.name(), "backend-a");
    }

    #[test]
    fn swap_backend() {
        let registry = ServiceRegistry::new();

        let a: Arc<dyn DummyService> = Arc::new(DummyBackendA);
        registry.register::<dyn DummyService>("dummy", "a", "@test/a", a);

        let b: Arc<dyn DummyService> = Arc::new(DummyBackendB);
        registry.register::<dyn DummyService>("dummy", "b", "@test/b", b);

        let retrieved = registry.get::<dyn DummyService>().unwrap();
        assert_eq!(retrieved.name(), "backend-b");

        let entries = registry.list();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].backend_id, "b");
    }

    #[test]
    fn missing_service() {
        let registry = ServiceRegistry::new();
        let result = registry.get::<dyn DummyService>();
        assert!(result.is_none());
    }
}
