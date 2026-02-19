//! Unit tests for resource usage calculations.

use nexus::agent::types::ResourceUsage;

// T024: Unit test for resource_usage() VRAM calculation
#[test]
fn test_vram_free_bytes_calculation() {
    // Test with both total and used set
    let usage = ResourceUsage {
        vram_total_bytes: Some(24_000_000_000), // 24GB
        vram_used_bytes: Some(8_000_000_000),   // 8GB
        pending_requests: None,
        avg_latency_ms: None,
        loaded_models: vec![],
    };
    assert_eq!(usage.vram_free_bytes(), Some(16_000_000_000)); // 16GB free
}

#[test]
fn test_vram_free_bytes_with_no_total() {
    // Test when total is None
    let usage = ResourceUsage {
        vram_total_bytes: None,
        vram_used_bytes: Some(8_000_000_000),
        pending_requests: None,
        avg_latency_ms: None,
        loaded_models: vec![],
    };
    assert_eq!(usage.vram_free_bytes(), None);
}

#[test]
fn test_vram_free_bytes_with_no_used() {
    // Test when used is None
    let usage = ResourceUsage {
        vram_total_bytes: Some(24_000_000_000),
        vram_used_bytes: None,
        pending_requests: None,
        avg_latency_ms: None,
        loaded_models: vec![],
    };
    assert_eq!(usage.vram_free_bytes(), None);
}

#[test]
fn test_vram_free_bytes_with_both_none() {
    // Test when both are None
    let usage = ResourceUsage::default();
    assert_eq!(usage.vram_free_bytes(), None);
}

#[test]
fn test_vram_free_bytes_saturating_sub() {
    // Test when used > total (shouldn't happen but be defensive)
    let usage = ResourceUsage {
        vram_total_bytes: Some(8_000_000_000),
        vram_used_bytes: Some(24_000_000_000),
        pending_requests: None,
        avg_latency_ms: None,
        loaded_models: vec![],
    };
    assert_eq!(usage.vram_free_bytes(), Some(0)); // Saturates to 0
}

#[test]
fn test_vram_free_bytes_zero_used() {
    // Test with zero usage (all free)
    let usage = ResourceUsage {
        vram_total_bytes: Some(24_000_000_000),
        vram_used_bytes: Some(0),
        pending_requests: None,
        avg_latency_ms: None,
        loaded_models: vec![],
    };
    assert_eq!(usage.vram_free_bytes(), Some(24_000_000_000)); // All free
}
