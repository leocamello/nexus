//! Unit tests for ActionableErrorContext serialization (T052-T053)

use nexus::api::error::{ActionableErrorContext, ServiceUnavailableError};

#[test]
fn test_actionable_error_context_serialization_full() {
    // T052: Test complete ActionableErrorContext with all fields
    let context = ActionableErrorContext {
        required_tier: Some(5),
        available_backends: vec!["openai-gpt4".to_string(), "anthropic-claude".to_string()],
        eta_seconds: Some(120),
        privacy_zone_required: Some("restricted".to_string()),
    };

    let json = serde_json::to_value(&context).unwrap();

    assert_eq!(json["required_tier"], 5);
    assert_eq!(json["available_backends"][0], "openai-gpt4");
    assert_eq!(json["available_backends"][1], "anthropic-claude");
    assert_eq!(json["eta_seconds"], 120);
    assert_eq!(json["privacy_zone_required"], "restricted");
}

#[test]
fn test_actionable_error_context_serialization_minimal() {
    // T052: Test ActionableErrorContext with only required fields
    let context = ActionableErrorContext {
        required_tier: None,
        available_backends: vec![],
        eta_seconds: None,
        privacy_zone_required: None,
    };

    let json = serde_json::to_value(&context).unwrap();

    // Optional fields should be omitted when None
    assert!(json.get("required_tier").is_none());
    assert!(json.get("eta_seconds").is_none());
    assert!(json.get("privacy_zone_required").is_none());

    // available_backends is always present (may be empty)
    assert_eq!(json["available_backends"].as_array().unwrap().len(), 0);
}

#[test]
fn test_actionable_error_context_deserialization() {
    // T052: Verify round-trip serialization
    let original = ActionableErrorContext {
        required_tier: Some(3),
        available_backends: vec!["backend1".to_string()],
        eta_seconds: Some(60),
        privacy_zone_required: None,
    };

    let json_str = serde_json::to_string(&original).unwrap();
    let deserialized: ActionableErrorContext = serde_json::from_str(&json_str).unwrap();

    assert_eq!(deserialized.required_tier, Some(3));
    assert_eq!(deserialized.available_backends, vec!["backend1"]);
    assert_eq!(deserialized.eta_seconds, Some(60));
    assert_eq!(deserialized.privacy_zone_required, None);
}

#[test]
fn test_service_unavailable_error_new() {
    // T053: Test ServiceUnavailableError::new() constructor
    let context = ActionableErrorContext {
        required_tier: Some(4),
        available_backends: vec!["backend1".to_string()],
        eta_seconds: None,
        privacy_zone_required: None,
    };

    let error = ServiceUnavailableError::new("Test error message".to_string(), context.clone());

    assert_eq!(error.error.message, "Test error message");
    assert_eq!(error.error.r#type, "service_unavailable");
    assert_eq!(error.error.code, Some("service_unavailable".to_string()));
    assert_eq!(error.context.required_tier, Some(4));
    assert_eq!(error.context.available_backends, vec!["backend1"]);
}

#[test]
fn test_service_unavailable_error_serialization() {
    // T053: Verify ServiceUnavailableError serializes to correct format
    let error = ServiceUnavailableError::tier_unavailable(5, vec!["openai-gpt4".to_string()]);

    let json = serde_json::to_value(&error).unwrap();

    // Check error envelope structure
    assert_eq!(json["error"]["type"], "service_unavailable");
    assert!(json["error"]["message"]
        .as_str()
        .unwrap()
        .contains("tier 5"));
    assert_eq!(json["error"]["code"], "service_unavailable");

    // Check context structure
    assert_eq!(json["context"]["required_tier"], 5);
    assert_eq!(json["context"]["available_backends"][0], "openai-gpt4");
}

#[test]
fn test_service_unavailable_error_deserialization() {
    // T053: Verify ServiceUnavailableError can be deserialized from JSON
    let json_str = r#"{
        "error": {
            "message": "No backends available",
            "type": "service_unavailable",
            "code": "service_unavailable",
            "param": null
        },
        "context": {
            "available_backends": [],
            "required_tier": 3
        }
    }"#;

    let error: ServiceUnavailableError = serde_json::from_str(json_str).unwrap();

    assert_eq!(error.error.message, "No backends available");
    assert_eq!(error.error.r#type, "service_unavailable");
    assert_eq!(error.context.required_tier, Some(3));
    assert!(error.context.available_backends.is_empty());
}

#[test]
fn test_service_unavailable_error_with_eta() {
    // T053: Test error with eta_seconds populated
    let context = ActionableErrorContext {
        required_tier: None,
        available_backends: vec![],
        eta_seconds: Some(300), // 5 minutes
        privacy_zone_required: None,
    };

    let error =
        ServiceUnavailableError::new("Backend temporarily unavailable".to_string(), context);

    let json = serde_json::to_value(&error).unwrap();
    assert_eq!(json["context"]["eta_seconds"], 300);
}

#[test]
fn test_service_unavailable_error_format_matches_spec() {
    // T053: Verify error format matches OpenAI + context extension
    let error = ServiceUnavailableError::all_backends_down();
    let json = serde_json::to_value(&error).unwrap();

    // Must have standard OpenAI error structure
    assert!(json.get("error").is_some());
    assert!(json["error"].get("message").is_some());
    assert!(json["error"].get("type").is_some());

    // Must have Nexus context extension
    assert!(json.get("context").is_some());
    assert!(json["context"].get("available_backends").is_some());
}
