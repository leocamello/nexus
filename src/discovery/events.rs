//! Discovery event types

use std::collections::HashMap;
use std::net::IpAddr;

/// Events emitted during mDNS service discovery
#[derive(Debug, Clone)]
pub enum DiscoveryEvent {
    /// A new service was discovered
    ServiceFound {
        /// mDNS instance name (e.g., "my-ollama._ollama._tcp.local")
        instance: String,
        /// Service type (e.g., "_ollama._tcp.local")
        service_type: String,
        /// IP addresses where the service is available
        addresses: Vec<IpAddr>,
        /// Port number
        port: u16,
        /// TXT records containing service metadata
        txt_records: HashMap<String, String>,
    },
    /// A previously discovered service was removed
    ServiceRemoved {
        /// mDNS instance name
        instance: String,
        /// Service type
        service_type: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{Ipv4Addr, Ipv6Addr};

    #[test]
    fn test_discovery_event_service_found_debug() {
        let event = DiscoveryEvent::ServiceFound {
            instance: "test-server".to_string(),
            service_type: "_ollama._tcp.local".to_string(),
            addresses: vec![IpAddr::V4(Ipv4Addr::new(192, 168, 1, 10))],
            port: 11434,
            txt_records: HashMap::new(),
        };
        let debug = format!("{:?}", event);
        assert!(debug.contains("ServiceFound"));
        assert!(debug.contains("test-server"));
    }

    #[test]
    fn test_discovery_event_service_removed_debug() {
        let event = DiscoveryEvent::ServiceRemoved {
            instance: "test-server".to_string(),
            service_type: "_ollama._tcp.local".to_string(),
        };
        let debug = format!("{:?}", event);
        assert!(debug.contains("ServiceRemoved"));
        assert!(debug.contains("test-server"));
    }

    #[test]
    fn test_service_info_with_ipv4_and_ipv6() {
        let event = DiscoveryEvent::ServiceFound {
            instance: "multi-ip-server".to_string(),
            service_type: "_llm._tcp.local".to_string(),
            addresses: vec![
                IpAddr::V4(Ipv4Addr::new(192, 168, 1, 10)),
                IpAddr::V6(Ipv6Addr::LOCALHOST),
            ],
            port: 8080,
            txt_records: HashMap::new(),
        };

        // Verify both addresses are stored
        match event {
            DiscoveryEvent::ServiceFound { addresses, .. } => {
                assert_eq!(addresses.len(), 2);
                assert!(addresses.contains(&IpAddr::V4(Ipv4Addr::new(192, 168, 1, 10))));
                assert!(addresses.contains(&IpAddr::V6(Ipv6Addr::LOCALHOST)));
            }
            _ => panic!("Expected ServiceFound event"),
        }
    }
}
