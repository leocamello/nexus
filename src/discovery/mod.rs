//! mDNS service discovery module
//!
//! This module provides automatic discovery of LLM backends using mDNS (multicast DNS).
//! It monitors the network for services advertising themselves via mDNS protocols
//! and automatically registers them with the backend registry.

mod events;
mod parser;

pub use events::*;
pub use parser::*;

// Re-export DiscoveryConfig from config module
pub use crate::config::DiscoveryConfig;

use crate::registry::{Backend, DiscoverySource, Registry};
use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::Mutex;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

/// Convert a DiscoveryEvent to a Backend instance
fn service_event_to_backend(event: &DiscoveryEvent) -> Option<Backend> {
    let DiscoveryEvent::ServiceFound {
        instance,
        service_type,
        addresses,
        port,
        txt_records,
    } = event
    else {
        return None;
    };

    // Must have at least one address
    if addresses.is_empty() {
        return None;
    }

    // Parse TXT records
    let parsed = parse_txt_records(txt_records, service_type);

    // Select best IP address (prefer IPv4)
    let selected_ip = select_best_ip(addresses);

    // Build URL
    let url = build_url(selected_ip, *port, &parsed.api_path);

    // Generate a human-readable name from instance
    let name = extract_name_from_instance(instance);

    // Generate unique ID
    let id = uuid::Uuid::new_v4().to_string();

    // Create metadata
    let mut metadata = HashMap::new();
    if let Some(version) = &parsed.version {
        metadata.insert("version".to_string(), version.clone());
    }
    metadata.insert("mdns_instance".to_string(), instance.clone());

    Some(Backend::new(
        id,
        name,
        url,
        parsed.backend_type,
        vec![], // Models will be discovered via health check
        DiscoverySource::MDNS,
        metadata,
    ))
}

/// Select the best IP address (prefer IPv4 over IPv6)
fn select_best_ip(addresses: &[IpAddr]) -> IpAddr {
    addresses
        .iter()
        .find(|ip| ip.is_ipv4())
        .or_else(|| addresses.first())
        .copied()
        .unwrap()
}

/// Build URL from IP, port, and API path
fn build_url(ip: IpAddr, port: u16, api_path: &str) -> String {
    let host = match ip {
        IpAddr::V4(addr) => addr.to_string(),
        IpAddr::V6(addr) => format!("[{}]", addr),
    };

    if api_path.is_empty() {
        format!("http://{}:{}", host, port)
    } else {
        format!("http://{}:{}{}", host, port, api_path)
    }
}

/// Extract a human-readable name from mDNS instance
fn extract_name_from_instance(instance: &str) -> String {
    // Instance format is typically "name._service._tcp.local"
    // Extract just the name part
    instance
        .split('.')
        .next()
        .unwrap_or(instance)
        .replace('_', " ")
        .trim()
        .to_string()
}

/// Main mDNS discovery service
pub struct MdnsDiscovery {
    config: DiscoveryConfig,
    registry: Arc<Registry>,
    pending_removal: Arc<Mutex<HashMap<String, Instant>>>,
    /// Shared HTTP client for creating agents (T028)
    client: Arc<reqwest::Client>,
}

impl MdnsDiscovery {
    /// Create a new MdnsDiscovery instance
    pub fn new(config: DiscoveryConfig, registry: Arc<Registry>) -> Self {
        // Create HTTP client with reasonable defaults for mDNS backends (T028)
        let client = Arc::new(
            reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(30))
                .build()
                .expect("Failed to build HTTP client for mDNS discovery"),
        );

        Self {
            config,
            registry,
            pending_removal: Arc::new(Mutex::new(HashMap::new())),
            client,
        }
    }

    /// Start the discovery service
    ///
    /// Returns a JoinHandle that can be awaited for graceful shutdown
    pub fn start(self, cancel_token: CancellationToken) -> JoinHandle<()> {
        tokio::spawn(async move {
            if !self.config.enabled {
                tracing::info!("mDNS discovery disabled");
                return;
            }

            // Main discovery loop (implemented in T10)
            self.run(cancel_token).await;
        })
    }

    /// Run the main discovery loop
    async fn run(self, cancel_token: CancellationToken) {
        // Try to create mDNS daemon
        let daemon = match mdns_sd::ServiceDaemon::new() {
            Ok(d) => d,
            Err(e) => {
                tracing::warn!("mDNS unavailable, discovery disabled: {}", e);
                return;
            }
        };

        tracing::info!("mDNS service daemon started");

        // Browse for each service type
        let mut receivers = Vec::new();
        for service_type in &self.config.service_types {
            // Normalize service type - mdns-sd requires trailing dot
            let normalized = if service_type.ends_with('.') {
                service_type.clone()
            } else {
                format!("{}.", service_type)
            };
            match daemon.browse(&normalized) {
                Ok(receiver) => {
                    tracing::info!(service_type = %service_type, "Browsing for mDNS service");
                    receivers.push((service_type.clone(), receiver));
                }
                Err(e) => {
                    tracing::error!(service_type = %service_type, error = %e, "Failed to browse for service");
                }
            }
        }

        if receivers.is_empty() {
            tracing::warn!("No mDNS service types could be browsed");
            return;
        }

        // Spawn cleanup task
        let cleanup_registry = self.registry.clone();
        let cleanup_config = self.config.clone();
        let cleanup_pending = self.pending_removal.clone();
        let cleanup_cancel = cancel_token.clone();

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(10));
            loop {
                tokio::select! {
                    _ = cleanup_cancel.cancelled() => break,
                    _ = interval.tick() => {
                        let temp_client = Arc::new(
                            reqwest::Client::builder()
                                .timeout(std::time::Duration::from_secs(30))
                                .build()
                                .expect("Failed to build HTTP client")
                        );
                        let temp_discovery = MdnsDiscovery {
                            config: cleanup_config.clone(),
                            registry: cleanup_registry.clone(),
                            pending_removal: cleanup_pending.clone(),
                            client: temp_client,
                        };
                        temp_discovery.cleanup_stale_backends().await;
                    }
                }
            }
        });

        // Main event loop
        loop {
            // Check cancellation first
            if cancel_token.is_cancelled() {
                tracing::info!("mDNS discovery shutting down");
                break;
            }

            // Poll all receivers for events
            for (_service_type, receiver) in &receivers {
                // Non-blocking check for events
                if let Ok(event) = receiver.try_recv() {
                    self.handle_mdns_event(event).await;
                }
            }

            // Small delay to avoid busy loop
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }

        // Shutdown daemon
        if let Err(e) = daemon.shutdown() {
            tracing::warn!("Error shutting down mDNS daemon: {}", e);
        }
    }

    /// Handle an mdns-sd ServiceEvent
    async fn handle_mdns_event(&self, event: mdns_sd::ServiceEvent) {
        match event {
            mdns_sd::ServiceEvent::ServiceResolved(info) => {
                // Convert to our DiscoveryEvent
                let addresses: Vec<IpAddr> = info.get_addresses().iter().copied().collect();
                let txt_records: HashMap<String, String> = info
                    .get_properties()
                    .iter()
                    .map(|prop| {
                        let key = prop.key().to_string();
                        let val = prop.val_str().to_string();
                        (key, val)
                    })
                    .collect();

                let discovery_event = DiscoveryEvent::ServiceFound {
                    instance: info.get_fullname().to_string(),
                    service_type: info.get_type().to_string(),
                    addresses,
                    port: info.get_port(),
                    txt_records,
                };

                self.handle_event(discovery_event).await;
            }
            mdns_sd::ServiceEvent::ServiceRemoved(_type, fullname) => {
                let discovery_event = DiscoveryEvent::ServiceRemoved {
                    instance: fullname.clone(),
                    service_type: _type.clone(),
                };

                self.handle_event(discovery_event).await;
            }
            mdns_sd::ServiceEvent::SearchStarted(_) => {
                // Informational, no action needed
            }
            mdns_sd::ServiceEvent::SearchStopped(_) => {
                // Informational, no action needed
            }
            _ => {
                // Other events we don't care about
            }
        }
    }

    /// Handle a discovery event.
    ///
    /// This method processes discovery events and updates the registry accordingly.
    /// Made public for integration testing.
    pub async fn handle_event(&self, event: DiscoveryEvent) {
        match event {
            DiscoveryEvent::ServiceFound { .. } => {
                self.handle_service_found(event).await;
            }
            DiscoveryEvent::ServiceRemoved { ref instance, .. } => {
                self.handle_service_removed(instance).await;
            }
        }
    }

    /// Handle a service found event
    async fn handle_service_found(&self, event: DiscoveryEvent) {
        let DiscoveryEvent::ServiceFound { ref instance, .. } = event else {
            return;
        };

        // Remove from pending removal if present (service has reappeared)
        self.pending_removal.lock().await.remove(instance);

        // Convert to backend
        let Some(backend) = service_event_to_backend(&event) else {
            tracing::warn!(instance = %instance, "Could not convert service to backend");
            return;
        };

        // Check if URL already exists
        if self.registry.has_backend_url(&backend.url) {
            tracing::debug!(url = %backend.url, "Backend URL already exists, skipping");
            return;
        }

        // Create agent for this backend (T028)
        let agent = match crate::agent::factory::create_agent(
            backend.id.clone(),
            backend.name.clone(),
            backend.url.clone(),
            backend.backend_type,
            Arc::clone(&self.client),
            backend.metadata.clone(),
            backend.backend_type.default_privacy_zone(),
            None,
        ) {
            Ok(agent) => agent,
            Err(e) => {
                tracing::error!(
                    instance = %instance,
                    error = %e,
                    "Failed to create agent for discovered backend"
                );
                return;
            }
        };

        // Add to registry with agent (T028)
        match self.registry.add_backend_with_agent(backend, agent) {
            Ok(()) => {
                // Find the backend we just added
                if let Some(id) = self
                    .registry
                    .get_all_backends()
                    .iter()
                    .find(|b| b.metadata.get("mdns_instance").map(|s| s.as_str()) == Some(instance))
                    .map(|b| b.id.clone())
                {
                    tracing::info!(
                        instance = %instance,
                        url = %self.registry.get_backend(&id).map(|b| b.url.to_string()).unwrap_or_default(),
                        "Discovered backend via mDNS"
                    );
                }
            }
            Err(e) => {
                tracing::error!(instance = %instance, error = %e, "Failed to add discovered backend");
            }
        }
    }

    /// Handle a service removed event (stub for now)
    async fn handle_service_removed(&self, instance: &str) {
        // Find backend by mDNS instance
        let Some(id) = self.registry.find_by_mdns_instance(instance) else {
            tracing::debug!(instance = %instance, "Service removed but not in registry");
            return;
        };

        // Set status to Unknown
        if let Err(e) = self.registry.update_status(
            &id,
            crate::registry::BackendStatus::Unknown,
            Some("Service disappeared from mDNS".to_string()),
        ) {
            tracing::error!(instance = %instance, error = %e, "Failed to update backend status");
            return;
        }

        // Add to pending removal
        self.pending_removal
            .lock()
            .await
            .insert(instance.to_string(), Instant::now());

        tracing::warn!(
            instance = %instance,
            grace_period_seconds = self.config.grace_period_seconds,
            "Backend disappeared from mDNS, starting grace period"
        );
    }

    /// Cleanup stale backends past their grace period.
    ///
    /// Removes backends that have been in pending_removal longer than the grace period.
    /// Made public for integration testing.
    pub async fn cleanup_stale_backends(&self) {
        let grace_period = std::time::Duration::from_secs(self.config.grace_period_seconds);
        let now = Instant::now();

        let mut pending = self.pending_removal.lock().await;
        let expired: Vec<String> = pending
            .iter()
            .filter(|(_, &removal_time)| now.duration_since(removal_time) > grace_period)
            .map(|(instance, _)| instance.clone())
            .collect();

        for instance in expired {
            pending.remove(&instance);

            if let Some(id) = self.registry.find_by_mdns_instance(&instance) {
                if let Err(e) = self.registry.remove_backend(&id) {
                    tracing::error!(instance = %instance, error = %e, "Failed to remove stale backend");
                } else {
                    tracing::info!(instance = %instance, "Removed stale mDNS backend after grace period");
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::BackendType;
    use std::net::{Ipv4Addr, Ipv6Addr};

    fn create_test_event(
        instance: &str,
        addresses: Vec<IpAddr>,
        port: u16,
        txt_records: HashMap<String, String>,
    ) -> DiscoveryEvent {
        DiscoveryEvent::ServiceFound {
            instance: instance.to_string(),
            service_type: "_ollama._tcp.local".to_string(),
            addresses,
            port,
            txt_records,
        }
    }

    #[test]
    fn test_service_to_backend_basic() {
        let event = create_test_event(
            "ollama-server",
            vec![IpAddr::V4(Ipv4Addr::new(192, 168, 1, 10))],
            11434,
            HashMap::new(),
        );
        let backend = service_event_to_backend(&event).unwrap();
        assert_eq!(backend.url, "http://192.168.1.10:11434");
        assert_eq!(backend.backend_type, BackendType::Ollama);
    }

    #[test]
    fn test_service_to_backend_with_api_path() {
        let mut txt = HashMap::new();
        txt.insert("api_path".to_string(), "/v1".to_string());
        let event = create_test_event(
            "vllm-server",
            vec![IpAddr::V4(Ipv4Addr::new(192, 168, 1, 10))],
            8000,
            txt,
        );
        let backend = service_event_to_backend(&event).unwrap();
        assert_eq!(backend.url, "http://192.168.1.10:8000/v1");
    }

    #[test]
    fn test_service_to_backend_prefers_ipv4() {
        let event = create_test_event(
            "test-server",
            vec![
                IpAddr::V6(Ipv6Addr::new(0xfe80, 0, 0, 0, 0, 0, 0, 1)),
                IpAddr::V4(Ipv4Addr::new(192, 168, 1, 10)),
            ],
            11434,
            HashMap::new(),
        );
        let backend = service_event_to_backend(&event).unwrap();
        assert!(backend.url.contains("192.168.1.10"));
    }

    #[test]
    fn test_service_to_backend_ipv6_only() {
        let event = create_test_event(
            "ipv6-server",
            vec![IpAddr::V6(Ipv6Addr::LOCALHOST)],
            11434,
            HashMap::new(),
        );
        let backend = service_event_to_backend(&event).unwrap();
        assert!(backend.url.contains("[::1]"));
    }

    #[test]
    fn test_service_to_backend_discovery_source() {
        let event = create_test_event(
            "test-server",
            vec![IpAddr::V4(Ipv4Addr::new(192, 168, 1, 10))],
            11434,
            HashMap::new(),
        );
        let backend = service_event_to_backend(&event).unwrap();
        assert_eq!(backend.discovery_source, DiscoverySource::MDNS);
    }

    #[test]
    fn test_service_to_backend_generates_name() {
        let event = create_test_event(
            "My_Ollama_Server._ollama._tcp.local",
            vec![IpAddr::V4(Ipv4Addr::new(192, 168, 1, 10))],
            11434,
            HashMap::new(),
        );
        let backend = service_event_to_backend(&event).unwrap();
        assert_eq!(backend.name, "My Ollama Server");
    }

    #[test]
    fn test_service_to_backend_no_addresses() {
        let event = create_test_event("test-server", vec![], 11434, HashMap::new());
        let backend = service_event_to_backend(&event);
        assert!(backend.is_none());
    }

    // MdnsDiscovery tests

    #[test]
    fn test_mdns_discovery_new() {
        let registry = Arc::new(Registry::new());
        let config = DiscoveryConfig::default();
        let _discovery = MdnsDiscovery::new(config, registry);
        // Should not panic
    }

    #[tokio::test]
    async fn test_mdns_discovery_disabled_returns_immediately() {
        let registry = Arc::new(Registry::new());
        let config = DiscoveryConfig {
            enabled: false,
            ..Default::default()
        };

        let discovery = MdnsDiscovery::new(config, registry);
        let cancel = CancellationToken::new();

        let handle = discovery.start(cancel.clone());

        // Should complete almost immediately since disabled
        let result = tokio::time::timeout(std::time::Duration::from_millis(100), handle).await;

        assert!(
            result.is_ok(),
            "Discovery should return immediately when disabled"
        );
    }

    #[tokio::test]
    async fn test_mdns_discovery_responds_to_cancellation() {
        let registry = Arc::new(Registry::new());
        let config = DiscoveryConfig::default();
        let discovery = MdnsDiscovery::new(config, registry);
        let cancel = CancellationToken::new();

        let handle = discovery.start(cancel.clone());

        // Cancel after short delay
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        cancel.cancel();

        // Should complete within reasonable time
        let result = tokio::time::timeout(std::time::Duration::from_secs(1), handle).await;

        assert!(
            result.is_ok(),
            "Discovery should respond to cancellation token"
        );
    }

    // Service found handler tests

    fn create_test_discovery(registry: Arc<Registry>) -> MdnsDiscovery {
        let config = DiscoveryConfig::default();
        MdnsDiscovery::new(config, registry)
    }

    fn create_found_event(instance: &str) -> DiscoveryEvent {
        DiscoveryEvent::ServiceFound {
            instance: instance.to_string(),
            service_type: "_ollama._tcp.local".to_string(),
            addresses: vec![IpAddr::V4(Ipv4Addr::new(192, 168, 1, 10))],
            port: 11434,
            txt_records: HashMap::new(),
        }
    }

    #[tokio::test]
    async fn test_handle_service_found_adds_backend() {
        let registry = Arc::new(Registry::new());
        let discovery = create_test_discovery(registry.clone());

        let event = create_found_event("test-ollama");

        discovery.handle_event(event).await;

        assert_eq!(registry.backend_count(), 1);
        assert!(registry.has_backend_url("http://192.168.1.10:11434"));
    }

    #[tokio::test]
    async fn test_handle_service_found_skips_existing_url() {
        let registry = Arc::new(Registry::new());

        // Pre-add a static backend
        let static_backend = Backend::new(
            uuid::Uuid::new_v4().to_string(),
            "Static Backend".to_string(),
            "http://192.168.1.10:11434".to_string(),
            BackendType::Ollama,
            vec![],
            DiscoverySource::Static,
            HashMap::new(),
        );
        registry.add_backend(static_backend).unwrap();

        let discovery = create_test_discovery(registry.clone());

        let event = create_found_event("mdns-ollama");

        discovery.handle_event(event).await;

        // Should still be just 1 backend (static one preserved)
        assert_eq!(registry.backend_count(), 1);
    }

    #[tokio::test]
    async fn test_handle_service_found_sets_mdns_instance() {
        let registry = Arc::new(Registry::new());
        let discovery = create_test_discovery(registry.clone());

        let event = DiscoveryEvent::ServiceFound {
            instance: "my-server._ollama._tcp.local".to_string(),
            service_type: "_ollama._tcp.local".to_string(),
            addresses: vec![IpAddr::V4(Ipv4Addr::new(192, 168, 1, 10))],
            port: 11434,
            txt_records: HashMap::new(),
        };

        discovery.handle_event(event).await;

        let id = registry
            .find_by_mdns_instance("my-server._ollama._tcp.local")
            .unwrap();
        assert!(!id.is_empty());
    }

    // Service removed handler tests

    fn create_removed_event(instance: &str) -> DiscoveryEvent {
        DiscoveryEvent::ServiceRemoved {
            instance: instance.to_string(),
            service_type: "_ollama._tcp.local".to_string(),
        }
    }

    #[tokio::test]
    async fn test_handle_service_removed_sets_unknown() {
        let registry = Arc::new(Registry::new());
        let discovery = create_test_discovery(registry.clone());

        // First discover the service
        discovery
            .handle_event(create_found_event("test-service"))
            .await;
        let id = registry.find_by_mdns_instance("test-service").unwrap();

        // Then remove it
        discovery
            .handle_event(create_removed_event("test-service"))
            .await;

        let backend = registry.get_backend(&id).unwrap();
        assert_eq!(backend.status, crate::registry::BackendStatus::Unknown);
    }

    #[tokio::test]
    async fn test_handle_service_removed_starts_grace_period() {
        let registry = Arc::new(Registry::new());
        let discovery = create_test_discovery(registry.clone());

        discovery
            .handle_event(create_found_event("test-service"))
            .await;
        discovery
            .handle_event(create_removed_event("test-service"))
            .await;

        // Backend should still exist during grace period
        assert!(registry.find_by_mdns_instance("test-service").is_some());

        // Check pending_removal has entry
        let pending = discovery.pending_removal.lock().await;
        assert!(pending.contains_key("test-service"));
    }

    #[tokio::test]
    async fn test_grace_period_expiry_removes_backend() {
        let registry = Arc::new(Registry::new());
        let config = DiscoveryConfig {
            grace_period_seconds: 1, // Short for testing
            ..Default::default()
        };
        let discovery = MdnsDiscovery::new(config, registry.clone());

        discovery
            .handle_event(create_found_event("test-service"))
            .await;
        discovery
            .handle_event(create_removed_event("test-service"))
            .await;

        // Wait for grace period + cleanup cycle
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        discovery.cleanup_stale_backends().await;

        // Backend should be removed
        assert!(registry.find_by_mdns_instance("test-service").is_none());
    }

    #[tokio::test]
    async fn test_service_reappears_cancels_removal() {
        let registry = Arc::new(Registry::new());
        let discovery = create_test_discovery(registry.clone());

        // Discover -> Remove -> Rediscover
        discovery
            .handle_event(create_found_event("test-service"))
            .await;
        discovery
            .handle_event(create_removed_event("test-service"))
            .await;
        discovery
            .handle_event(create_found_event("test-service"))
            .await;

        // Should not be in pending_removal
        let pending = discovery.pending_removal.lock().await;
        assert!(!pending.contains_key("test-service"));

        // Backend should still exist
        assert!(registry.find_by_mdns_instance("test-service").is_some());
    }

    #[tokio::test]
    async fn test_cleanup_only_removes_mdns_backends() {
        let registry = Arc::new(Registry::new());

        // Add a static backend
        let static_backend = Backend::new(
            uuid::Uuid::new_v4().to_string(),
            "Static Backend".to_string(),
            "http://localhost:8080".to_string(),
            BackendType::Ollama,
            vec![],
            DiscoverySource::Static,
            HashMap::new(),
        );
        registry.add_backend(static_backend).unwrap();

        let discovery = create_test_discovery(registry.clone());

        // Discover and remove an mDNS backend
        discovery
            .handle_event(create_found_event("mdns-service"))
            .await;

        // Manually expire it immediately
        let config = DiscoveryConfig {
            grace_period_seconds: 0, // Expire immediately
            ..Default::default()
        };
        let discovery2 = MdnsDiscovery::new(config, registry.clone());
        discovery2
            .handle_event(create_removed_event("mdns-service"))
            .await;

        // Wait a moment
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;

        // Run cleanup
        discovery2.cleanup_stale_backends().await;

        // Static backend should remain
        assert_eq!(registry.backend_count(), 1);
        let backends = registry.get_all_backends();
        let backend = backends.first().unwrap();
        assert_eq!(backend.discovery_source, DiscoverySource::Static);
    }

    #[test]
    fn test_select_best_ip_prefers_ipv4() {
        let addrs = vec![
            "::1".parse::<IpAddr>().unwrap(),
            "192.168.1.100".parse::<IpAddr>().unwrap(),
        ];
        assert_eq!(
            select_best_ip(&addrs),
            "192.168.1.100".parse::<IpAddr>().unwrap()
        );
    }

    #[test]
    fn test_select_best_ip_falls_back_to_ipv6() {
        let addrs = vec!["::1".parse::<IpAddr>().unwrap()];
        assert_eq!(select_best_ip(&addrs), "::1".parse::<IpAddr>().unwrap());
    }

    #[test]
    fn test_build_url_ipv4() {
        let ip = "192.168.1.1".parse::<IpAddr>().unwrap();
        assert_eq!(build_url(ip, 11434, ""), "http://192.168.1.1:11434");
    }

    #[test]
    fn test_build_url_ipv4_with_path() {
        let ip = "192.168.1.1".parse::<IpAddr>().unwrap();
        assert_eq!(build_url(ip, 8000, "/v1"), "http://192.168.1.1:8000/v1");
    }

    #[test]
    fn test_build_url_ipv6() {
        let ip = "::1".parse::<IpAddr>().unwrap();
        assert_eq!(build_url(ip, 11434, ""), "http://[::1]:11434");
    }

    #[test]
    fn test_extract_name_from_instance_simple() {
        assert_eq!(
            extract_name_from_instance("my-ollama._http._tcp.local"),
            "my-ollama"
        );
    }

    #[test]
    fn test_extract_name_from_instance_with_underscores() {
        assert_eq!(
            extract_name_from_instance("my_ollama._http._tcp.local"),
            "my ollama"
        );
    }

    #[test]
    fn test_extract_name_from_instance_no_dots() {
        assert_eq!(extract_name_from_instance("simple-name"), "simple-name");
    }
}
