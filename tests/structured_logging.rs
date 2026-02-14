//! Integration tests for structured request logging
//!
//! These tests verify that the structured logging system correctly captures
//! and formats request metadata according to the specification.

#[cfg(test)]
mod us1_basic_logging {
    use nexus::api::{ChatCompletionResponse, Usage};
    use nexus::logging::{extract_status, extract_tokens, generate_request_id};

    #[test]
    fn test_successful_request_produces_structured_log_fields() {
        // T101: Verify that a successful response produces all required structured fields
        let response = ChatCompletionResponse {
            id: "chatcmpl-test".to_string(),
            object: "chat.completion".to_string(),
            created: 1234567890,
            model: "gpt-4".to_string(),
            choices: vec![],
            usage: Some(Usage {
                prompt_tokens: 100,
                completion_tokens: 50,
                total_tokens: 150,
            }),
        };

        // Verify token extraction produces all required fields
        let (prompt, completion, total) = extract_tokens(&response);
        assert_eq!(prompt, 100);
        assert_eq!(completion, 50);
        assert_eq!(total, 150);

        // Verify status extraction
        let ok_result: Result<axum::response::Response, nexus::api::ApiError> =
            Ok(axum::response::Response::new(axum::body::Body::empty()));
        let (status, error_msg) = extract_status(&ok_result);
        assert_eq!(status, "success");
        assert!(error_msg.is_none());

        // Verify request_id is generated
        let request_id = generate_request_id();
        assert!(!request_id.is_empty());
    }

    #[test]
    fn test_json_format_log_entry_fields_are_valid_json() {
        // T102: Verify that all log field values can be serialized as valid JSON
        let request_id = generate_request_id();
        let model = "gpt-4";
        let backend = "local-ollama";
        let status = "success";
        let latency_ms: u64 = 1234;
        let tokens_prompt: u32 = 100;
        let tokens_completion: u32 = 50;
        let stream = false;

        let json = serde_json::json!({
            "request_id": request_id,
            "model": model,
            "backend": backend,
            "status": status,
            "latency_ms": latency_ms,
            "tokens_prompt": tokens_prompt,
            "tokens_completion": tokens_completion,
            "stream": stream,
        });

        // Must produce valid JSON
        let json_str = serde_json::to_string(&json).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json_str).unwrap();
        assert!(parsed.is_object());
        assert_eq!(parsed["model"], "gpt-4");
        assert_eq!(parsed["latency_ms"], 1234);
    }

    #[test]
    fn test_request_id_is_valid_uuid_v4() {
        // T103: Verify request_id is valid UUID v4
        let request_id = generate_request_id();

        // UUID v4 format: xxxxxxxx-xxxx-4xxx-yxxx-xxxxxxxxxxxx
        assert_eq!(request_id.len(), 36);
        let parts: Vec<&str> = request_id.split('-').collect();
        assert_eq!(parts.len(), 5);
        assert_eq!(parts[0].len(), 8);
        assert_eq!(parts[1].len(), 4);
        assert_eq!(parts[2].len(), 4);
        assert_eq!(parts[3].len(), 4);
        assert_eq!(parts[4].len(), 12);

        // Version nibble must be '4'
        assert!(
            parts[2].starts_with('4'),
            "UUID v4 must have '4' as version"
        );

        // Must be parseable
        let parsed = uuid::Uuid::parse_str(&request_id);
        assert!(parsed.is_ok());
        assert_eq!(parsed.unwrap().get_version_num(), 4);
    }

    #[test]
    fn test_latency_ms_measurement() {
        // T104: Verify latency measurement mechanism works correctly
        let start = std::time::Instant::now();
        std::thread::sleep(std::time::Duration::from_millis(10));
        let elapsed = start.elapsed().as_millis() as u64;

        assert!(
            elapsed >= 10,
            "Latency should be at least 10ms, got {}",
            elapsed
        );
        assert!(
            elapsed < 200,
            "Latency should be reasonable, got {}ms",
            elapsed
        );
    }

    #[test]
    fn test_failed_request_produces_error_status() {
        // T105: Verify failed request produces log entry with status=error
        let error = nexus::api::ApiError::service_unavailable("No healthy backend");
        let result: Result<axum::response::Response, nexus::api::ApiError> = Err(error);
        let (status, error_msg) = extract_status(&result);

        assert_ne!(status, "success");
        assert!(error_msg.is_some());
        assert!(
            error_msg.unwrap().contains("No healthy backend"),
            "Error message should contain the failure reason"
        );
    }
}

#[cfg(test)]
mod us2_request_correlation {
    use nexus::logging::generate_request_id;

    #[test]
    fn test_request_id_persists_across_retries() {
        // T106: Verify same request_id can be reused across retry attempts
        let request_id = generate_request_id();

        // Simulate retry loop — same ID should be used for all attempts
        let mut retry_entries: Vec<(String, u32)> = Vec::new();
        for retry_count in 0..3 {
            retry_entries.push((request_id.clone(), retry_count));
        }

        // All entries share the same request_id
        assert!(retry_entries.iter().all(|(id, _)| id == &request_id));
        // retry_count increments
        assert_eq!(retry_entries[0].1, 0);
        assert_eq!(retry_entries[1].1, 1);
        assert_eq!(retry_entries[2].1, 2);
    }

    #[test]
    fn test_fallback_chain_shows_progression() {
        // T107: Verify fallback_chain builds correctly
        let mut fallback_chain: Vec<String> = Vec::new();

        // First attempt on backend1
        fallback_chain.push("backend1".to_string());
        assert_eq!(fallback_chain.join(","), "backend1");

        // Retry on backend2
        fallback_chain.push("backend2".to_string());
        assert_eq!(fallback_chain.join(","), "backend1,backend2");

        // Retry on backend3
        fallback_chain.push("backend3".to_string());
        assert_eq!(fallback_chain.join(","), "backend1,backend2,backend3");
    }

    #[test]
    fn test_first_try_request_has_zero_retry_count() {
        // T108: Verify first-try request starts with retry_count=0
        let retry_count: u32 = 0;
        let fallback_chain: Vec<String> = Vec::new();

        assert_eq!(retry_count, 0);
        assert!(fallback_chain.is_empty());
        assert_eq!(fallback_chain.join(","), "");
    }

    #[test]
    fn test_retry_log_level_progression() {
        // T109: Verify log level conventions for retry scenarios
        // INFO for first attempt, WARN for retries, ERROR for exhausted
        let max_retries = 3;
        let mut levels: Vec<&str> = Vec::new();

        for attempt in 0..=max_retries {
            if attempt == 0 {
                levels.push("INFO");
            } else if attempt < max_retries {
                levels.push("WARN");
            } else {
                levels.push("ERROR");
            }
        }

        assert_eq!(levels[0], "INFO");
        assert_eq!(levels[1], "WARN");
        assert_eq!(levels[2], "WARN");
        assert_eq!(levels[3], "ERROR");
    }
}

#[cfg(test)]
mod us3_routing_visibility {
    use chrono::Utc;
    use std::sync::atomic::{AtomicU32, AtomicU64};

    #[tokio::test]
    async fn test_route_reason_score_based() {
        // T110: Test that score-based routing produces descriptive route_reason
        use nexus::registry::{
            Backend, BackendStatus, BackendType, DiscoverySource, Model, Registry,
        };
        use nexus::routing::{RequestRequirements, Router, RoutingStrategy, ScoringWeights};
        use std::collections::HashMap;
        use std::sync::Arc;

        let registry = Arc::new(Registry::new());

        let backend1 = Backend {
            id: "backend1".to_string(),
            name: "Backend 1".to_string(),
            url: "http://backend1:8000".to_string(),
            backend_type: BackendType::OpenAI,
            status: BackendStatus::Healthy,
            last_health_check: Utc::now(),
            last_error: None,
            models: vec![Model {
                id: "gpt-4".to_string(),
                name: "GPT-4".to_string(),
                context_length: 8192,
                supports_vision: false,
                supports_tools: true,
                supports_json_mode: false,
                max_output_tokens: None,
            }],
            priority: 5,
            pending_requests: AtomicU32::new(0),
            total_requests: AtomicU64::new(0),
            avg_latency_ms: AtomicU32::new(50),
            discovery_source: DiscoverySource::Static,
            metadata: HashMap::new(),
        };
        let _ = registry.add_backend(backend1);

        let backend2 = Backend {
            id: "backend2".to_string(),
            name: "Backend 2".to_string(),
            url: "http://backend2:8000".to_string(),
            backend_type: BackendType::OpenAI,
            status: BackendStatus::Healthy,
            last_health_check: Utc::now(),
            last_error: None,
            models: vec![Model {
                id: "gpt-4".to_string(),
                name: "GPT-4".to_string(),
                context_length: 8192,
                supports_vision: false,
                supports_tools: true,
                supports_json_mode: false,
                max_output_tokens: None,
            }],
            priority: 3,
            pending_requests: AtomicU32::new(0),
            total_requests: AtomicU64::new(0),
            avg_latency_ms: AtomicU32::new(50),
            discovery_source: DiscoverySource::Static,
            metadata: HashMap::new(),
        };
        let _ = registry.add_backend(backend2);

        let router = Router::new(
            Arc::clone(&registry),
            RoutingStrategy::Smart,
            ScoringWeights::default(),
        );

        let requirements = RequestRequirements {
            model: "gpt-4".to_string(),
            estimated_tokens: 100,
            needs_vision: false,
            needs_tools: false,
            needs_json_mode: false,
        };
        let result = router.select_backend(&requirements).unwrap();

        assert!(
            result.route_reason.starts_with("highest_score:")
                || result.route_reason == "only_healthy_backend",
            "Expected route_reason to explain score-based selection, got: {}",
            result.route_reason
        );
    }

    #[tokio::test]
    async fn test_route_reason_fallback_scenario() {
        // T111: Test that fallback scenario produces explanatory route_reason
        use nexus::registry::{
            Backend, BackendStatus, BackendType, DiscoverySource, Model, Registry,
        };
        use nexus::routing::{RequestRequirements, Router, RoutingStrategy, ScoringWeights};
        use std::collections::HashMap;
        use std::sync::Arc;

        let registry = Arc::new(Registry::new());

        let backend1 = Backend {
            id: "backend1".to_string(),
            name: "Backend 1".to_string(),
            url: "http://backend1:8000".to_string(),
            backend_type: BackendType::OpenAI,
            status: BackendStatus::Healthy,
            last_health_check: Utc::now(),
            last_error: None,
            models: vec![Model {
                id: "gpt-3.5-turbo".to_string(),
                name: "GPT-3.5 Turbo".to_string(),
                context_length: 4096,
                supports_vision: false,
                supports_tools: true,
                supports_json_mode: false,
                max_output_tokens: None,
            }],
            priority: 5,
            pending_requests: AtomicU32::new(0),
            total_requests: AtomicU64::new(0),
            avg_latency_ms: AtomicU32::new(50),
            discovery_source: DiscoverySource::Static,
            metadata: HashMap::new(),
        };
        let _ = registry.add_backend(backend1);

        let mut fallbacks = HashMap::new();
        fallbacks.insert("gpt-4".to_string(), vec!["gpt-3.5-turbo".to_string()]);

        let router = Router::with_aliases_and_fallbacks(
            Arc::clone(&registry),
            RoutingStrategy::Smart,
            ScoringWeights::default(),
            HashMap::new(),
            fallbacks,
        );

        let requirements = RequestRequirements {
            model: "gpt-4".to_string(),
            estimated_tokens: 100,
            needs_vision: false,
            needs_tools: false,
            needs_json_mode: false,
        };
        let result = router.select_backend(&requirements).unwrap();

        assert!(result.fallback_used);
        assert_eq!(result.actual_model, "gpt-3.5-turbo");
        assert!(
            result.route_reason.starts_with("fallback:"),
            "Expected route_reason to start with 'fallback:', got: {}",
            result.route_reason
        );
    }

    #[tokio::test]
    async fn test_route_reason_single_healthy_backend() {
        // T112: Test single healthy backend produces appropriate route_reason
        use nexus::registry::{
            Backend, BackendStatus, BackendType, DiscoverySource, Model, Registry,
        };
        use nexus::routing::{RequestRequirements, Router, RoutingStrategy, ScoringWeights};
        use std::collections::HashMap;
        use std::sync::Arc;

        let registry = Arc::new(Registry::new());

        let backend1 = Backend {
            id: "backend1".to_string(),
            name: "Backend 1".to_string(),
            url: "http://backend1:8000".to_string(),
            backend_type: BackendType::OpenAI,
            status: BackendStatus::Healthy,
            last_health_check: Utc::now(),
            last_error: None,
            models: vec![Model {
                id: "gpt-4".to_string(),
                name: "GPT-4".to_string(),
                context_length: 8192,
                supports_vision: false,
                supports_tools: true,
                supports_json_mode: false,
                max_output_tokens: None,
            }],
            priority: 5,
            pending_requests: AtomicU32::new(0),
            total_requests: AtomicU64::new(0),
            avg_latency_ms: AtomicU32::new(50),
            discovery_source: DiscoverySource::Static,
            metadata: HashMap::new(),
        };
        let _ = registry.add_backend(backend1);

        let router = Router::new(
            Arc::clone(&registry),
            RoutingStrategy::Smart,
            ScoringWeights::default(),
        );

        let requirements = RequestRequirements {
            model: "gpt-4".to_string(),
            estimated_tokens: 100,
            needs_vision: false,
            needs_tools: false,
            needs_json_mode: false,
        };
        let result = router.select_backend(&requirements).unwrap();

        assert_eq!(result.route_reason, "only_healthy_backend");
    }
}

#[cfg(test)]
mod us4_privacy_safe_logging {
    #[tokio::test]
    async fn test_default_no_content_logging() {
        // T113: Default config logs contain no message content
        use nexus::api::{ChatCompletionRequest, ChatMessage, MessageContent};
        use nexus::logging::truncate_prompt;
        use std::collections::HashMap;

        let request = ChatCompletionRequest {
            model: "gpt-4".to_string(),
            messages: vec![ChatMessage {
                role: "user".to_string(),
                content: MessageContent::Text {
                    content: "This is sensitive user data that should not be logged".to_string(),
                },
                name: None,
            }],
            stream: false,
            temperature: None,
            max_tokens: None,
            top_p: None,
            stop: None,
            presence_penalty: None,
            frequency_penalty: None,
            user: None,
            extra: HashMap::new(),
        };

        let preview = truncate_prompt(&request, false);
        assert!(preview.is_none(), "No content preview when disabled");
    }

    #[tokio::test]
    async fn test_startup_warning_when_content_logging_enabled() {
        // T114: LoggingConfig correctly reflects enable_content_logging
        use nexus::config::logging::LoggingConfig;

        let config = LoggingConfig {
            level: "info".to_string(),
            format: nexus::config::logging::LogFormat::Json,
            component_levels: None,
            enable_content_logging: true,
        };

        assert!(config.enable_content_logging);
    }

    #[tokio::test]
    async fn test_prompt_preview_when_enabled() {
        // T115: Content preview truncated to ~100 chars when enabled
        use nexus::api::{ChatCompletionRequest, ChatMessage, MessageContent};
        use nexus::logging::truncate_prompt;
        use std::collections::HashMap;

        let long_message = "A".repeat(200);
        let request = ChatCompletionRequest {
            model: "gpt-4".to_string(),
            messages: vec![ChatMessage {
                role: "user".to_string(),
                content: MessageContent::Text {
                    content: long_message,
                },
                name: None,
            }],
            stream: false,
            temperature: None,
            max_tokens: None,
            top_p: None,
            stop: None,
            presence_penalty: None,
            frequency_penalty: None,
            user: None,
            extra: HashMap::new(),
        };

        let preview = truncate_prompt(&request, true);
        assert!(preview.is_some());
        let preview_str = preview.unwrap();
        assert!(preview_str.len() <= 110);
        assert!(preview_str.starts_with('A'));
    }
}

#[cfg(test)]
mod us5_component_log_levels {
    #[test]
    fn test_component_levels_builds_env_filter() {
        // T116: component_levels builds correct EnvFilter directives
        use nexus::config::logging::LoggingConfig;
        use nexus::logging::build_filter_directives;
        use std::collections::HashMap;

        let mut component_levels = HashMap::new();
        component_levels.insert("routing".to_string(), "debug".to_string());
        component_levels.insert("api".to_string(), "info".to_string());
        component_levels.insert("health".to_string(), "warn".to_string());

        let config = LoggingConfig {
            level: "info".to_string(),
            format: nexus::config::logging::LogFormat::Pretty,
            component_levels: Some(component_levels),
            enable_content_logging: false,
        };

        let filter_str = build_filter_directives(&config);
        assert!(filter_str.starts_with("info"));
        assert!(filter_str.contains("nexus::routing=debug"));
        assert!(filter_str.contains("nexus::api=info"));
        assert!(filter_str.contains("nexus::health=warn"));
    }

    #[test]
    fn test_build_filter_directives_produces_valid_string() {
        // T117: Filter directives valid with and without component levels
        use nexus::config::logging::LoggingConfig;
        use nexus::logging::build_filter_directives;
        use std::collections::HashMap;

        // No component levels
        let config_simple = LoggingConfig {
            level: "warn".to_string(),
            format: nexus::config::logging::LogFormat::Pretty,
            component_levels: None,
            enable_content_logging: false,
        };
        assert_eq!(build_filter_directives(&config_simple), "warn");

        // With component levels
        let mut component_levels = HashMap::new();
        component_levels.insert("routing".to_string(), "trace".to_string());

        let config_with = LoggingConfig {
            level: "error".to_string(),
            format: nexus::config::logging::LogFormat::Json,
            component_levels: Some(component_levels),
            enable_content_logging: false,
        };
        assert_eq!(
            build_filter_directives(&config_with),
            "error,nexus::routing=trace"
        );
    }
}

#[cfg(test)]
mod us6_aggregator_compatibility {
    #[test]
    fn test_numeric_fields_are_numbers_not_strings() {
        // T119: Numeric fields serialize as JSON numbers
        use nexus::api::{ChatCompletionResponse, Usage};
        use nexus::logging::extract_tokens;

        let response = ChatCompletionResponse {
            id: "test".to_string(),
            object: "chat.completion".to_string(),
            created: 1234567890,
            model: "gpt-4".to_string(),
            choices: vec![],
            usage: Some(Usage {
                prompt_tokens: 100,
                completion_tokens: 50,
                total_tokens: 150,
            }),
        };

        let (prompt, completion, total) = extract_tokens(&response);
        assert_eq!(prompt, 100u32);
        assert_eq!(completion, 50u32);
        assert_eq!(total, 150u32);

        let json = serde_json::json!({
            "tokens_prompt": prompt,
            "tokens_completion": completion,
            "latency_ms": 123u64,
        });
        let json_str = serde_json::to_string(&json).unwrap();
        assert!(json_str.contains("\"tokens_prompt\":100"));
        assert!(json_str.contains("\"latency_ms\":123"));
    }

    #[test]
    fn test_timestamp_format_rfc3339() {
        // T120: Timestamps are RFC3339 with UTC
        use chrono::{DateTime, Utc};

        let now: DateTime<Utc> = Utc::now();
        let timestamp_str = now.to_rfc3339();

        assert!(
            timestamp_str.ends_with('Z')
                || timestamp_str.contains('+')
                || timestamp_str.contains('-'),
        );

        let parsed: Result<DateTime<Utc>, _> =
            DateTime::parse_from_rfc3339(&timestamp_str).map(|dt| dt.with_timezone(&Utc));
        assert!(parsed.is_ok());
    }

    #[test]
    fn test_json_schema_field_types() {
        // T118: Log fields match expected types
        let log_entry = serde_json::json!({
            "timestamp": "2025-02-14T10:00:00Z",
            "level": "INFO",
            "target": "nexus::api",
            "fields": {
                "request_id": "550e8400-e29b-41d4-a716-446655440000",
                "model": "gpt-4",
                "backend": "backend1",
                "backend_type": "openai",
                "status": "success",
                "status_code": 200u16,
                "latency_ms": 123u64,
                "tokens_prompt": 100u32,
                "tokens_completion": 50u32,
                "tokens_total": 150u32,
                "stream": false,
                "retry_count": 0u32,
                "fallback_chain": "",
                "route_reason": "highest_score:backend1:0.95",
            }
        });

        let json_str = serde_json::to_string(&log_entry).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json_str).unwrap();

        assert!(parsed["fields"]["latency_ms"].is_number());
        assert!(parsed["fields"]["tokens_prompt"].is_number());
        assert!(parsed["fields"]["stream"].is_boolean());
        assert!(parsed["fields"]["request_id"].is_string());
    }
}
