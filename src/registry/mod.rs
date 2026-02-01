//! Backend Registry module.
//!
//! Provides thread-safe in-memory storage and querying of LLM backends.

mod backend;
mod error;
#[cfg(test)]
mod tests;

pub use backend::*;
pub use error::*;

use dashmap::DashMap;

/// The Backend Registry stores all known LLM backends.
///
/// Thread-safe registry for managing LLM inference backends. Uses lock-free
/// concurrent maps (DashMap) for high-performance reads and writes.
///
/// # Examples
///
/// ```
/// use nexus::registry::{Registry, Backend, BackendType, DiscoverySource};
/// use std::collections::HashMap;
///
/// let registry = Registry::new();
///
/// let backend = Backend::new(
///     "backend-1".to_string(),
///     "My Backend".to_string(),
///     "http://localhost:11434".to_string(),
///     BackendType::Ollama,
///     vec![],
///     DiscoverySource::Static,
///     HashMap::new(),
/// );
///
/// registry.add_backend(backend).unwrap();
/// assert_eq!(registry.backend_count(), 1);
/// ```
pub struct Registry {
    backends: DashMap<String, Backend>,
    model_index: DashMap<String, Vec<String>>,
}

impl Registry {
    /// Create a new empty Registry.
    pub fn new() -> Self {
        Self {
            backends: DashMap::new(),
            model_index: DashMap::new(),
        }
    }

    /// Add a new backend to the registry.
    ///
    /// # Errors
    ///
    /// Returns `RegistryError::DuplicateBackend` if a backend with the same ID already exists.
    ///
    /// # Examples
    ///
    /// ```
    /// use nexus::registry::{Registry, Backend, BackendType, DiscoverySource};
    /// use std::collections::HashMap;
    ///
    /// let registry = Registry::new();
    /// let backend = Backend::new(
    ///     "backend-1".to_string(),
    ///     "My Backend".to_string(),
    ///     "http://localhost:11434".to_string(),
    ///     BackendType::Ollama,
    ///     vec![],
    ///     DiscoverySource::Static,
    ///     HashMap::new(),
    /// );
    ///
    /// assert!(registry.add_backend(backend).is_ok());
    /// ```
    pub fn add_backend(&self, backend: Backend) -> Result<(), RegistryError> {
        let id = backend.id.clone();

        // Check for duplicate
        if self.backends.contains_key(&id) {
            return Err(RegistryError::DuplicateBackend(id));
        }

        // Update model index
        for model in &backend.models {
            self.model_index
                .entry(model.id.clone())
                .or_default()
                .push(id.clone());
        }

        // Insert backend
        self.backends.insert(id, backend);
        Ok(())
    }

    /// Remove a backend from the registry.
    ///
    /// Also cleans up the model index to remove references to this backend.
    ///
    /// # Errors
    ///
    /// Returns `RegistryError::BackendNotFound` if no backend with the given ID exists.
    pub fn remove_backend(&self, id: &str) -> Result<Backend, RegistryError> {
        let backend = self
            .backends
            .remove(id)
            .map(|(_, backend)| backend)
            .ok_or_else(|| RegistryError::BackendNotFound(id.to_string()))?;

        // Cleanup model index
        for model in &backend.models {
            if let Some(mut backend_ids) = self.model_index.get_mut(&model.id) {
                backend_ids.retain(|bid| bid != id);
                // If the list is empty, we'll remove the entry
                if backend_ids.is_empty() {
                    drop(backend_ids); // Release the lock before removing
                    self.model_index.remove(&model.id);
                }
            }
        }

        Ok(backend)
    }

    /// Get a backend by ID.
    ///
    /// Returns a cloned copy of the backend (including atomic counter values).
    pub fn get_backend(&self, id: &str) -> Option<Backend> {
        self.backends.get(id).map(|entry| {
            let backend = entry.value();
            // Clone all fields to create a new Backend
            Backend {
                id: backend.id.clone(),
                name: backend.name.clone(),
                url: backend.url.clone(),
                backend_type: backend.backend_type,
                status: backend.status,
                last_health_check: backend.last_health_check,
                last_error: backend.last_error.clone(),
                models: backend.models.clone(),
                priority: backend.priority,
                pending_requests: std::sync::atomic::AtomicU32::new(
                    backend
                        .pending_requests
                        .load(std::sync::atomic::Ordering::SeqCst),
                ),
                total_requests: std::sync::atomic::AtomicU64::new(
                    backend
                        .total_requests
                        .load(std::sync::atomic::Ordering::SeqCst),
                ),
                avg_latency_ms: std::sync::atomic::AtomicU32::new(
                    backend
                        .avg_latency_ms
                        .load(std::sync::atomic::Ordering::SeqCst),
                ),
                discovery_source: backend.discovery_source,
                metadata: backend.metadata.clone(),
            }
        })
    }

    /// Get all backends.
    ///
    /// Returns cloned copies of all registered backends.
    pub fn get_all_backends(&self) -> Vec<Backend> {
        self.backends
            .iter()
            .map(|entry| {
                let backend = entry.value();
                Backend {
                    id: backend.id.clone(),
                    name: backend.name.clone(),
                    url: backend.url.clone(),
                    backend_type: backend.backend_type,
                    status: backend.status,
                    last_health_check: backend.last_health_check,
                    last_error: backend.last_error.clone(),
                    models: backend.models.clone(),
                    priority: backend.priority,
                    pending_requests: std::sync::atomic::AtomicU32::new(
                        backend
                            .pending_requests
                            .load(std::sync::atomic::Ordering::SeqCst),
                    ),
                    total_requests: std::sync::atomic::AtomicU64::new(
                        backend
                            .total_requests
                            .load(std::sync::atomic::Ordering::SeqCst),
                    ),
                    avg_latency_ms: std::sync::atomic::AtomicU32::new(
                        backend
                            .avg_latency_ms
                            .load(std::sync::atomic::Ordering::SeqCst),
                    ),
                    discovery_source: backend.discovery_source,
                    metadata: backend.metadata.clone(),
                }
            })
            .collect()
    }

    /// Get the number of registered backends.
    pub fn backend_count(&self) -> usize {
        self.backends.len()
    }

    /// Get all backends.
    ///
    /// Returns cloned copies of all registered backends. that serve a specific model
    pub fn get_backends_for_model(&self, model_id: &str) -> Vec<Backend> {
        if let Some(backend_ids) = self.model_index.get(model_id) {
            backend_ids
                .iter()
                .filter_map(|id| self.get_backend(id))
                .collect()
        } else {
            Vec::new()
        }
    }

    /// Get all backends.
    ///
    /// Returns cloned copies of all registered backends. with Healthy status
    pub fn get_healthy_backends(&self) -> Vec<Backend> {
        self.backends
            .iter()
            .filter(|entry| entry.value().status == BackendStatus::Healthy)
            .map(|entry| {
                let backend = entry.value();
                Backend {
                    id: backend.id.clone(),
                    name: backend.name.clone(),
                    url: backend.url.clone(),
                    backend_type: backend.backend_type,
                    status: backend.status,
                    last_health_check: backend.last_health_check,
                    last_error: backend.last_error.clone(),
                    models: backend.models.clone(),
                    priority: backend.priority,
                    pending_requests: std::sync::atomic::AtomicU32::new(
                        backend
                            .pending_requests
                            .load(std::sync::atomic::Ordering::SeqCst),
                    ),
                    total_requests: std::sync::atomic::AtomicU64::new(
                        backend
                            .total_requests
                            .load(std::sync::atomic::Ordering::SeqCst),
                    ),
                    avg_latency_ms: std::sync::atomic::AtomicU32::new(
                        backend
                            .avg_latency_ms
                            .load(std::sync::atomic::Ordering::SeqCst),
                    ),
                    discovery_source: backend.discovery_source,
                    metadata: backend.metadata.clone(),
                }
            })
            .collect()
    }

    /// Get the number of unique models across all backends.
    pub fn model_count(&self) -> usize {
        self.model_index.len()
    }

    /// Update the health status of a backend.
    ///
    /// Sets the status, updates last_health_check timestamp, and sets/clears last_error.
    pub fn update_status(
        &self,
        id: &str,
        status: BackendStatus,
        error: Option<String>,
    ) -> Result<(), RegistryError> {
        let mut backend = self
            .backends
            .get_mut(id)
            .ok_or_else(|| RegistryError::BackendNotFound(id.to_string()))?;

        backend.status = status;
        backend.last_health_check = chrono::Utc::now();
        backend.last_error = error;

        Ok(())
    }

    /// Update the model list for a backend.
    ///
    /// Replaces the entire model list and updates the model index accordingly.
    pub fn update_models(&self, id: &str, new_models: Vec<Model>) -> Result<(), RegistryError> {
        let mut backend = self
            .backends
            .get_mut(id)
            .ok_or_else(|| RegistryError::BackendNotFound(id.to_string()))?;

        // Remove old models from index
        for model in &backend.models {
            if let Some(mut backend_ids) = self.model_index.get_mut(&model.id) {
                backend_ids.retain(|bid| bid != id);
                if backend_ids.is_empty() {
                    drop(backend_ids);
                    self.model_index.remove(&model.id);
                }
            }
        }

        // Replace models
        backend.models = new_models.clone();

        // Add new models to index
        for model in &new_models {
            self.model_index
                .entry(model.id.clone())
                .or_default()
                .push(id.to_string());
        }

        Ok(())
    }

    /// Atomically increment pending requests counter.
    ///
    /// Returns the new value after increment.
    pub fn increment_pending(&self, id: &str) -> Result<u32, RegistryError> {
        let backend = self
            .backends
            .get(id)
            .ok_or_else(|| RegistryError::BackendNotFound(id.to_string()))?;

        let new_val = backend
            .pending_requests
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst)
            + 1;
        Ok(new_val)
    }

    /// Atomically decrement pending requests counter (saturating at 0).
    ///
    /// If already at 0, logs a warning and returns 0.
    pub fn decrement_pending(&self, id: &str) -> Result<u32, RegistryError> {
        let backend = self
            .backends
            .get(id)
            .ok_or_else(|| RegistryError::BackendNotFound(id.to_string()))?;

        // Use compare-exchange loop for saturating subtraction
        loop {
            let current = backend
                .pending_requests
                .load(std::sync::atomic::Ordering::SeqCst);
            if current == 0 {
                tracing::warn!(
                    backend_id = %id,
                    "Attempted to decrement pending_requests when already at 0"
                );
                return Ok(0);
            }

            let new_val = current - 1;
            match backend.pending_requests.compare_exchange(
                current,
                new_val,
                std::sync::atomic::Ordering::SeqCst,
                std::sync::atomic::Ordering::SeqCst,
            ) {
                Ok(_) => return Ok(new_val),
                Err(_) => continue, // Retry if another thread modified the value
            }
        }
    }

    /// Update rolling average latency using EMA: new = (sample + 4*old) / 5.
    ///
    /// Uses integer math with α=0.2. First sample sets the initial value.
    pub fn update_latency(&self, id: &str, latency_ms: u32) -> Result<(), RegistryError> {
        let backend = self
            .backends
            .get(id)
            .ok_or_else(|| RegistryError::BackendNotFound(id.to_string()))?;

        loop {
            let current = backend
                .avg_latency_ms
                .load(std::sync::atomic::Ordering::SeqCst);

            // If this is the first sample (current == 0), just set it directly
            let new_val = if current == 0 {
                latency_ms
            } else {
                // EMA: new = (sample + 4*old) / 5
                (latency_ms + 4 * current) / 5
            };

            match backend.avg_latency_ms.compare_exchange(
                current,
                new_val,
                std::sync::atomic::Ordering::SeqCst,
                std::sync::atomic::Ordering::SeqCst,
            ) {
                Ok(_) => return Ok(()),
                Err(_) => continue, // Retry if another thread modified the value
            }
        }
    }
}

impl Default for Registry {
    fn default() -> Self {
        Self::new()
    }
}
