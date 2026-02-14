//! Structured logging module for request tracing
//!
//! This module provides utilities for structured logging of API requests,
//! including field extraction, request ID generation, and log formatting.

pub mod fields;
pub mod middleware;

pub use fields::{extract_status, extract_tokens, truncate_prompt};
pub use middleware::generate_request_id;

/// Build filter directives string from LoggingConfig
///
/// Constructs a tracing filter string that includes the base log level
/// and any component-specific log levels configured in the LoggingConfig.
///
/// # Arguments
///
/// * `config` - The logging configuration
///
/// # Returns
///
/// A filter string in the format: "base_level,nexus::component1=level1,nexus::component2=level2"
///
/// # Examples
///
/// ```no_run
/// use nexus::config::logging::LoggingConfig;
/// use nexus::logging::build_filter_directives;
/// use std::collections::HashMap;
///
/// let mut component_levels = HashMap::new();
/// component_levels.insert("routing".to_string(), "debug".to_string());
///
/// let config = LoggingConfig {
///     level: "info".to_string(),
///     format: nexus::config::logging::LogFormat::Pretty,
///     component_levels: Some(component_levels),
///     enable_content_logging: false,
/// };
///
/// let filter_str = build_filter_directives(&config);
/// assert_eq!(filter_str, "info,nexus::routing=debug");
/// ```
pub fn build_filter_directives(config: &crate::config::LoggingConfig) -> String {
    let mut filter_str = config.level.clone();

    if let Some(component_levels) = &config.component_levels {
        for (component, level) in component_levels {
            filter_str.push_str(&format!(",nexus::{}={}", component, level));
        }
    }

    filter_str
}
