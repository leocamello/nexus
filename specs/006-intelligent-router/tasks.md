# F06: Intelligent Router - Implementation Tasks

**Feature**: Intelligent Router  
**Spec**: [spec.md](./spec.md)  
**Plan**: [plan.md](./plan.md)  
**Created**: 2026-02-08

---

## Task Overview

| Task | Title | Est. Time | Dependencies |
|------|-------|-----------|--------------|
| T01 | Create routing module structure | 1h | None |
| T02 | Implement RequestRequirements | 2h | T01 |
| T03 | Implement candidate filtering | 2h | T01, T02 |
| T04 | Implement RoutingError types | 1h | T01 |
| T05 | Implement scoring function | 2h | T01 |
| T06 | Implement smart strategy | 2h | T03, T05 |
| T07 | Implement round-robin strategy | 1h | T03 |
| T08 | Implement priority-only strategy | 1h | T03 |
| T09 | Implement random strategy | 1h | T03 |
| T10 | Implement alias resolution | 2h | T06 |
| T11 | Implement fallback chains | 2h | T10 |
| T12 | Add RoutingConfig | 2h | T11 |
| T13 | Integrate with API handlers | 2h | T12 |
| T14 | Add integration tests | 2h | T13 |
| T15 | Performance validation | 1h | T14 |

**Total Estimated Time**: ~24 hours

---

## T01: Create Routing Module Structure

**Objective**: Set up the routing module directory and basic types

### Tests to Write First
```rust
// src/routing/mod.rs
#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn routing_strategy_default_is_smart() {
        assert_eq!(RoutingStrategy::default(), RoutingStrategy::Smart);
    }
    
    #[test]
    fn routing_strategy_from_str() {
        assert_eq!("smart".parse::<RoutingStrategy>().unwrap(), RoutingStrategy::Smart);
        assert_eq!("round_robin".parse::<RoutingStrategy>().unwrap(), RoutingStrategy::RoundRobin);
        assert_eq!("priority_only".parse::<RoutingStrategy>().unwrap(), RoutingStrategy::PriorityOnly);
        assert_eq!("random".parse::<RoutingStrategy>().unwrap(), RoutingStrategy::Random);
    }
}
```

### Implementation Steps
1. Create `src/routing/` directory
2. Create `mod.rs` with module declarations
3. Define `RoutingStrategy` enum with Default and FromStr
4. Add `pub mod routing;` to `src/lib.rs`

### Acceptance Criteria
- [X] `src/routing/mod.rs` exists with module structure
- [X] `RoutingStrategy` enum defined with all 4 variants
- [X] Default strategy is Smart
- [X] FromStr parses all strategy names (case-insensitive)
- [X] Module compiles without errors

---

## T02: Implement RequestRequirements

**Objective**: Extract routing requirements from incoming requests

### Tests to Write First
```rust
// src/routing/requirements.rs
#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn extracts_model_name() {
        let request = create_simple_request("llama3:8b", "Hello");
        let requirements = RequestRequirements::from_request(&request);
        assert_eq!(requirements.model, "llama3:8b");
    }
    
    #[test]
    fn estimates_tokens_from_content() {
        let request = create_simple_request("llama3:8b", "a]".repeat(1000));
        let requirements = RequestRequirements::from_request(&request);
        assert!(requirements.estimated_tokens >= 250); // 1000 chars / 4
    }
    
    #[test]
    fn detects_vision_requirement() {
        let request = create_vision_request("llava", "image_url_here");
        let requirements = RequestRequirements::from_request(&request);
        assert!(requirements.needs_vision);
    }
    
    #[test]
    fn detects_tools_requirement() {
        let request = create_tools_request("llama3:8b", vec!["get_weather"]);
        let requirements = RequestRequirements::from_request(&request);
        assert!(requirements.needs_tools);
    }
    
    #[test]
    fn detects_json_mode_requirement() {
        let request = create_json_mode_request("llama3:8b");
        let requirements = RequestRequirements::from_request(&request);
        assert!(requirements.needs_json_mode);
    }
    
    #[test]
    fn simple_request_has_no_special_requirements() {
        let request = create_simple_request("llama3:8b", "Hello");
        let requirements = RequestRequirements::from_request(&request);
        assert!(!requirements.needs_vision);
        assert!(!requirements.needs_tools);
        assert!(!requirements.needs_json_mode);
    }
}
```

### Implementation Steps
1. Create `src/routing/requirements.rs`
2. Define `RequestRequirements` struct
3. Implement `from_request()` method
4. Detect vision from `image_url` content type
5. Detect tools from `tools` array
6. Detect JSON mode from `response_format`
7. Estimate tokens from message content lengths

### Acceptance Criteria
- [X] `RequestRequirements` struct defined with all fields
- [X] Model name extracted correctly
- [X] Token estimation: characters / 4
- [X] Vision detected from `image_url` in any message content
- [X] Tools detected from non-empty `tools` array
- [X] JSON mode detected from `response_format.type == "json_object"`
- [X] All unit tests pass

---

## T03: Implement Candidate Filtering

**Objective**: Filter backends by model, health, and capabilities

### Tests to Write First
```rust
// src/routing/mod.rs
#[cfg(test)]
mod filter_tests {
    #[test]
    fn filters_by_model_name() {
        let registry = create_test_registry();
        // Add backend A with llama3, backend B with mistral
        let candidates = filter_candidates(&registry, "llama3:8b", &requirements);
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].name, "backend_a");
    }
    
    #[test]
    fn filters_out_unhealthy_backends() {
        let registry = create_test_registry();
        // Add healthy backend A, unhealthy backend B, both with llama3
        let candidates = filter_candidates(&registry, "llama3:8b", &requirements);
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].name, "backend_a");
    }
    
    #[test]
    fn filters_by_vision_capability() {
        let registry = create_test_registry();
        // Backend A has llama3 (no vision), backend B has llama3 (with vision)
        let requirements = RequestRequirements { needs_vision: true, ..default() };
        let candidates = filter_candidates(&registry, "llama3:8b", &requirements);
        assert_eq!(candidates.len(), 1);
        assert!(candidates[0].model.supports_vision);
    }
    
    #[test]
    fn filters_by_context_length() {
        let registry = create_test_registry();
        // Backend A has 4K context, backend B has 128K context
        let requirements = RequestRequirements { estimated_tokens: 10000, ..default() };
        let candidates = filter_candidates(&registry, "llama3:8b", &requirements);
        assert_eq!(candidates.len(), 1);
        assert!(candidates[0].model.context_length >= 10000);
    }
    
    #[test]
    fn returns_empty_when_no_match() {
        let registry = create_test_registry();
        let candidates = filter_candidates(&registry, "nonexistent", &requirements);
        assert!(candidates.is_empty());
    }
}
```

### Implementation Steps
1. Add `filter_candidates()` function to Router
2. Get backends for model from registry
3. Filter by `BackendStatus::Healthy`
4. Filter by vision capability if `needs_vision`
5. Filter by tools capability if `needs_tools`
6. Filter by context length >= estimated_tokens

### Acceptance Criteria
- [X] Returns only backends with matching model
- [X] Filters out unhealthy backends
- [X] Filters by vision capability when required
- [X] Filters by tools capability when required
- [X] Filters by context length when estimated_tokens > model.context_length
- [X] Returns empty Vec when no candidates match

---

## T04: Implement RoutingError Types

**Objective**: Define descriptive error types for routing failures

### Tests to Write First
```rust
// src/routing/error.rs
#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn model_not_found_error_message() {
        let error = RoutingError::ModelNotFound { model: "gpt-5".into() };
        assert_eq!(error.to_string(), "Model 'gpt-5' not found");
    }
    
    #[test]
    fn no_healthy_backend_error_message() {
        let error = RoutingError::NoHealthyBackend { model: "llama3:8b".into() };
        assert!(error.to_string().contains("No healthy backend"));
    }
    
    #[test]
    fn capability_mismatch_error_lists_missing() {
        let error = RoutingError::CapabilityMismatch {
            model: "llama3:8b".into(),
            missing: vec!["vision".into(), "tools".into()],
        };
        let msg = error.to_string();
        assert!(msg.contains("vision"));
        assert!(msg.contains("tools"));
    }
    
    #[test]
    fn fallback_exhausted_lists_chain() {
        let error = RoutingError::FallbackChainExhausted {
            chain: vec!["llama3:70b".into(), "mistral:7b".into()],
        };
        let msg = error.to_string();
        assert!(msg.contains("llama3:70b"));
        assert!(msg.contains("mistral:7b"));
    }
}
```

### Implementation Steps
1. Create `src/routing/error.rs`
2. Define `RoutingError` enum with thiserror
3. Implement Display for each variant
4. Export from mod.rs

### Acceptance Criteria
- [X] `RoutingError::ModelNotFound` with model name
- [X] `RoutingError::NoHealthyBackend` with model name
- [X] `RoutingError::CapabilityMismatch` with model and missing capabilities
- [X] `RoutingError::FallbackChainExhausted` with attempted chain
- [X] All errors implement std::error::Error
- [X] Error messages are descriptive and include relevant context

---

## T05: Implement Scoring Function

**Objective**: Score backends by priority, load, and latency

### Tests to Write First
```rust
// src/routing/scoring.rs
#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn default_weights_sum_to_100() {
        let weights = ScoringWeights::default();
        assert_eq!(weights.priority + weights.load + weights.latency, 100);
    }
    
    #[test]
    fn score_perfect_backend() {
        // Priority 0, 0 pending, 0ms latency
        let score = calculate_score(0, 0, 0, &ScoringWeights::default());
        assert_eq!(score, 100); // Maximum possible
    }
    
    #[test]
    fn score_worst_backend() {
        // Priority 100, 100 pending, 1000ms latency
        let score = calculate_score(100, 100, 1000, &ScoringWeights::default());
        assert_eq!(score, 0); // Minimum possible
    }
    
    #[test]
    fn priority_affects_score() {
        let low_priority = calculate_score(10, 50, 100, &ScoringWeights::default());
        let high_priority = calculate_score(90, 50, 100, &ScoringWeights::default());
        assert!(low_priority > high_priority);
    }
    
    #[test]
    fn load_affects_score() {
        let low_load = calculate_score(50, 10, 100, &ScoringWeights::default());
        let high_load = calculate_score(50, 90, 100, &ScoringWeights::default());
        assert!(low_load > high_load);
    }
    
    #[test]
    fn latency_affects_score() {
        let low_latency = calculate_score(50, 50, 10, &ScoringWeights::default());
        let high_latency = calculate_score(50, 50, 900, &ScoringWeights::default());
        assert!(low_latency > high_latency);
    }
    
    #[test]
    fn values_above_100_are_clamped() {
        let score = calculate_score(200, 200, 2000, &ScoringWeights::default());
        assert_eq!(score, 0); // All components clamped to min score
    }
}

// Property-based tests
#[cfg(test)]
mod prop_tests {
    use proptest::prelude::*;
    
    proptest! {
        #[test]
        fn score_always_in_range(
            priority in 0u32..200,
            pending in 0u32..200,
            latency in 0u32..2000,
        ) {
            let score = calculate_score(priority, pending, latency, &ScoringWeights::default());
            prop_assert!(score <= 100);
        }
    }
}
```

### Implementation Steps
1. Create `src/routing/scoring.rs`
2. Define `ScoringWeights` struct with Default
3. Implement `calculate_score()` function
4. Use integer math for performance
5. Clamp values > 100 to prevent underflow

### Acceptance Criteria
- [X] `ScoringWeights` with priority=50, load=30, latency=20 default
- [X] Score formula: `(priority_score * w.priority + load_score * w.load + latency_score * w.latency) / 100`
- [X] Priority score: `100 - min(priority, 100)`
- [X] Load score: `100 - min(pending_requests, 100)`
- [X] Latency score: `100 - min(avg_latency_ms / 10, 100)`
- [X] Score always in range 0-100
- [X] Property tests pass

---

## T06: Implement Smart Strategy

**Objective**: Select highest-scoring backend

### Tests to Write First
```rust
// src/routing/mod.rs
#[cfg(test)]
mod smart_strategy_tests {
    #[test]
    fn selects_highest_scoring_backend() {
        let router = create_router_with_candidates(vec![
            ("backend_a", 80), // lower score
            ("backend_b", 95), // highest score
            ("backend_c", 70), // lowest score
        ]);
        let selected = router.select_with_smart_strategy(&candidates);
        assert_eq!(selected.name, "backend_b");
    }
    
    #[test]
    fn selects_first_on_tie() {
        let router = create_router_with_candidates(vec![
            ("backend_a", 80),
            ("backend_b", 80),
        ]);
        let selected = router.select_with_smart_strategy(&candidates);
        assert_eq!(selected.name, "backend_a");
    }
    
    #[test]
    fn works_with_single_candidate() {
        let router = create_router_with_candidates(vec![
            ("backend_a", 50),
        ]);
        let selected = router.select_with_smart_strategy(&candidates);
        assert_eq!(selected.name, "backend_a");
    }
}
```

### Implementation Steps
1. Implement `select_with_smart_strategy()` on Router
2. Score each candidate
3. Return backend with highest score
4. On tie, return first (stable selection)

### Acceptance Criteria
- [X] Selects backend with highest score
- [X] Returns first backend on score tie (deterministic)
- [X] Works with single candidate
- [X] Scoring uses configured weights

---

## T07: Implement Round-Robin Strategy

**Objective**: Distribute requests evenly across backends

### Tests to Write First
```rust
// src/routing/mod.rs
#[cfg(test)]
mod round_robin_tests {
    #[test]
    fn cycles_through_backends() {
        let router = create_router_with_strategy(RoutingStrategy::RoundRobin);
        let candidates = vec![backend_a, backend_b, backend_c];
        
        assert_eq!(router.select_with_round_robin(&candidates).name, "backend_a");
        assert_eq!(router.select_with_round_robin(&candidates).name, "backend_b");
        assert_eq!(router.select_with_round_robin(&candidates).name, "backend_c");
        assert_eq!(router.select_with_round_robin(&candidates).name, "backend_a"); // wraps
    }
    
    #[test]
    fn handles_changing_candidate_list() {
        let router = create_router_with_strategy(RoutingStrategy::RoundRobin);
        
        // First call with 3 candidates
        let candidates1 = vec![backend_a, backend_b, backend_c];
        router.select_with_round_robin(&candidates1);
        
        // Second call with 2 candidates (one removed)
        let candidates2 = vec![backend_a, backend_c];
        let selected = router.select_with_round_robin(&candidates2);
        // Should still work (counter mod new_length)
        assert!(selected.name == "backend_a" || selected.name == "backend_c");
    }
    
    #[test]
    fn thread_safe_concurrent_access() {
        let router = Arc::new(create_router_with_strategy(RoutingStrategy::RoundRobin));
        let candidates = Arc::new(vec![backend_a, backend_b, backend_c]);
        
        let handles: Vec<_> = (0..100)
            .map(|_| {
                let r = router.clone();
                let c = candidates.clone();
                thread::spawn(move || r.select_with_round_robin(&c))
            })
            .collect();
        
        for h in handles {
            h.join().unwrap();
        }
        // No panic = thread safe
    }
}
```

### Implementation Steps
1. Add `AtomicU64` counter to Router
2. Implement `select_with_round_robin()`
3. Use `fetch_add` for atomic increment
4. Select candidate at `counter % candidates.len()`

### Acceptance Criteria
- [X] Cycles through all candidates in order
- [X] Wraps around after last candidate
- [X] Handles candidate list changes (uses modulo)
- [X] Thread-safe with concurrent access
- [X] Counter uses atomic operations

---

## T08: Implement Priority-Only Strategy

**Objective**: Always select lowest priority number

### Tests to Write First
```rust
// src/routing/mod.rs
#[cfg(test)]
mod priority_only_tests {
    #[test]
    fn selects_lowest_priority_number() {
        let candidates = vec![
            create_backend("a", 5),  // priority 5
            create_backend("b", 1),  // priority 1 (lowest)
            create_backend("c", 10), // priority 10
        ];
        let router = create_router_with_strategy(RoutingStrategy::PriorityOnly);
        let selected = router.select_with_priority_only(&candidates);
        assert_eq!(selected.name, "b");
    }
    
    #[test]
    fn selects_first_on_priority_tie() {
        let candidates = vec![
            create_backend("a", 1),
            create_backend("b", 1),
        ];
        let router = create_router_with_strategy(RoutingStrategy::PriorityOnly);
        let selected = router.select_with_priority_only(&candidates);
        assert_eq!(selected.name, "a");
    }
}
```

### Implementation Steps
1. Implement `select_with_priority_only()`
2. Find minimum priority value
3. Return first backend with that priority

### Acceptance Criteria
- [X] Selects backend with lowest priority number
- [X] Returns first on priority tie (stable)
- [X] Ignores load and latency

---

## T09: Implement Random Strategy

**Objective**: Random selection for testing/chaos

### Tests to Write First
```rust
// src/routing/mod.rs
#[cfg(test)]
mod random_tests {
    #[test]
    fn produces_varied_results() {
        let router = create_router_with_strategy(RoutingStrategy::Random);
        let candidates = vec![backend_a, backend_b, backend_c];
        
        let mut selections: HashMap<String, u32> = HashMap::new();
        for _ in 0..300 {
            let selected = router.select_with_random(&candidates);
            *selections.entry(selected.name.clone()).or_insert(0) += 1;
        }
        
        // Each backend should be selected at least once in 300 tries
        // (probability of not selecting one is (2/3)^300 ≈ 0)
        assert!(selections.len() == 3);
        
        // Roughly even distribution (each should get 80-120 out of 300)
        for count in selections.values() {
            assert!(*count > 50 && *count < 150);
        }
    }
}
```

### Implementation Steps
1. Add `fastrand::Rng` or use thread_rng
2. Implement `select_with_random()`
3. Generate random index in range
4. Return candidate at that index

### Acceptance Criteria
- [X] Selects randomly from candidates
- [X] Distribution approximately even over many calls
- [X] Works with any number of candidates

---

## T10: Implement Alias Resolution

**Objective**: Map model names to alternatives

### Tests to Write First
```rust
// src/routing/mod.rs
#[cfg(test)]
mod alias_tests {
    #[test]
    fn resolves_single_alias() {
        let router = create_router_with_aliases(hashmap! {
            "gpt-4" => "llama3:70b"
        });
        assert_eq!(router.resolve_alias("gpt-4"), "llama3:70b");
    }
    
    #[test]
    fn returns_original_if_no_alias() {
        let router = create_router_with_aliases(hashmap! {});
        assert_eq!(router.resolve_alias("llama3:8b"), "llama3:8b");
    }
    
    #[test]
    fn detects_circular_alias() {
        let router = create_router_with_aliases(hashmap! {
            "a" => "b",
            "b" => "c",
            "c" => "a"  // circular!
        });
        let result = router.resolve_alias_safe("a");
        assert!(result.is_err());
    }
    
    #[test]
    fn limits_alias_depth_to_single_level() {
        // Even if a->b and b->c are configured, requesting "a" only resolves to "b"
        let router = create_router_with_aliases(hashmap! {
            "a" => "b",
            "b" => "c"
        });
        // resolve_alias("a") returns "b", not "c"
        assert_eq!(router.resolve_alias("a"), "b");
    }
    
    #[test]
    fn single_level_alias_only() {
        // Aliases don't chain by design
        let router = create_router_with_aliases(hashmap! {
            "gpt-4" => "llama3:70b",
            "llama3:70b" => "mistral:7b"  // This won't be followed
        });
        // When requesting gpt-4, we get llama3:70b, not mistral:7b
        assert_eq!(router.resolve_alias("gpt-4"), "llama3:70b");
    }
}
```

### Implementation Steps
1. Add `aliases: HashMap<String, String>` to Router
2. Implement `resolve_alias()` - single level only
3. Implement `resolve_alias_safe()` - with circular detection for same-entry cycles
4. Use alias resolution in `select_backend()` when no candidates found

### Acceptance Criteria
- [X] Resolves configured aliases
- [X] Returns original model if no alias exists
- [X] Detects circular aliases (a→a same entry)
- [X] Aliases are single-level (a→b does not follow b→c)
- [X] Applied when model not found in registry

---

## T11: Implement Fallback Chains

**Objective**: Try alternative models when primary unavailable

### Tests to Write First
```rust
// src/routing/mod.rs
#[cfg(test)]
mod fallback_tests {
    #[test]
    fn tries_fallbacks_in_order() {
        let registry = create_registry_with_model("mistral:7b");
        let router = create_router_with_fallbacks(hashmap! {
            "llama3:70b" => vec!["llama3:8b", "mistral:7b"]
        });
        
        // llama3:70b not available, llama3:8b not available, mistral:7b available
        let result = router.select_backend_for_model("llama3:70b", &requirements);
        assert!(result.is_ok());
        // Should have selected mistral:7b via fallback
    }
    
    #[test]
    fn returns_error_when_all_fallbacks_exhausted() {
        let registry = create_empty_registry();
        let router = create_router_with_fallbacks(hashmap! {
            "model_a" => vec!["model_b", "model_c"]
        });
        
        let result = router.select_backend_for_model("model_a", &requirements);
        assert!(matches!(result, Err(RoutingError::FallbackChainExhausted { .. })));
    }
    
    #[test]
    fn fallbacks_are_single_level() {
        // model_a -> [model_b], model_b -> [model_c]
        // When requesting model_a and model_b unavailable, we don't try model_c
        let router = create_router_with_fallbacks(hashmap! {
            "model_a" => vec!["model_b"],
            "model_b" => vec!["model_c"]
        });
        // ... test that model_c is NOT tried
    }
    
    #[test]
    fn aliases_applied_before_fallbacks() {
        // alias: gpt-4 -> llama3:70b
        // fallback: llama3:70b -> [mistral:7b]
        // Request gpt-4 -> resolve to llama3:70b -> try fallback mistral:7b
        let router = create_router_with_aliases_and_fallbacks(...);
        // ...
    }
}
```

### Implementation Steps
1. Add `fallbacks: HashMap<String, Vec<String>>` to Router
2. Implement `get_fallback_chain()` method
3. In `select_backend()`, try fallbacks when no candidates
4. Return `FallbackChainExhausted` error if all fail
5. Fallbacks are single-level only

### Acceptance Criteria
- [X] Tries fallback models in order
- [X] Returns error when all fallbacks exhausted
- [X] Fallbacks are single-level (don't chain)
- [X] Aliases resolved before fallbacks applied
- [X] Error includes list of attempted models

---

## T12: Add RoutingConfig

**Objective**: Configuration for routing settings

### Tests to Write First
```rust
// src/config.rs
#[cfg(test)]
mod routing_config_tests {
    #[test]
    fn parses_routing_config() {
        let toml = r#"
            [routing]
            strategy = "round_robin"
            max_retries = 3
            
            [routing.weights]
            priority = 60
            load = 25
            latency = 15
            
            [routing.aliases]
            "gpt-4" = "llama3:70b"
            
            [routing.fallbacks]
            "llama3:70b" = ["llama3:8b", "mistral:7b"]
        "#;
        
        let config: NexusConfig = toml::from_str(toml).unwrap();
        
        assert_eq!(config.routing.strategy, RoutingStrategy::RoundRobin);
        assert_eq!(config.routing.max_retries, 3);
        assert_eq!(config.routing.weights.priority, 60);
        assert_eq!(config.routing.aliases.get("gpt-4"), Some(&"llama3:70b".into()));
        assert_eq!(config.routing.fallbacks.get("llama3:70b").unwrap().len(), 2);
    }
    
    #[test]
    fn default_routing_config() {
        let config = RoutingConfig::default();
        assert_eq!(config.strategy, RoutingStrategy::Smart);
        assert_eq!(config.max_retries, 2);
        assert_eq!(config.weights.priority, 50);
    }
    
    #[test]
    fn env_override_routing_strategy() {
        std::env::set_var("NEXUS_ROUTING_STRATEGY", "priority_only");
        let config = RoutingConfig::default().with_env_overrides();
        assert_eq!(config.strategy, RoutingStrategy::PriorityOnly);
        std::env::remove_var("NEXUS_ROUTING_STRATEGY");
    }
}
```

### Implementation Steps
1. Add `RoutingConfig` struct to config.rs
2. Add `ScoringWeights` with serde
3. Parse aliases as `HashMap<String, String>`
4. Parse fallbacks as `HashMap<String, Vec<String>>`
5. Add environment variable overrides
6. Add to `NexusConfig`

### Acceptance Criteria
- [X] `RoutingConfig` parses from TOML
- [X] Default values: strategy=Smart, max_retries=2, weights=50/30/20
- [X] Aliases parsed as string map
- [X] Fallbacks parsed as string-to-vec map
- [X] Environment overrides work for strategy and max_retries

---

## T13: Integrate with API Handlers

**Objective**: Use router in HTTP request handling

### Tests to Write First
```rust
// tests/api_routing_integration.rs
#[tokio::test]
async fn routes_request_to_correct_backend() {
    // Setup: Two backends with different models
    let mock_a = MockBackend::new("llama3:8b");
    let mock_b = MockBackend::new("mistral:7b");
    
    let app = create_test_app(vec![mock_a.addr(), mock_b.addr()]);
    
    // Request llama3:8b
    let response = app
        .post("/v1/chat/completions")
        .json(&json!({ "model": "llama3:8b", "messages": [...] }))
        .await;
    
    assert!(response.status().is_success());
    assert_eq!(mock_a.request_count(), 1);
    assert_eq!(mock_b.request_count(), 0);
}

#[tokio::test]
async fn returns_404_for_unknown_model() {
    let app = create_test_app(vec![]);
    
    let response = app
        .post("/v1/chat/completions")
        .json(&json!({ "model": "nonexistent", "messages": [...] }))
        .await;
    
    assert_eq!(response.status(), 404);
    let body: serde_json::Value = response.json().await;
    assert!(body["error"]["message"].as_str().unwrap().contains("not found"));
}

#[tokio::test]
async fn returns_503_when_no_healthy_backend() {
    // All backends unhealthy
    let app = create_test_app_unhealthy(vec![...]);
    
    let response = app
        .post("/v1/chat/completions")
        .json(&json!({ "model": "llama3:8b", "messages": [...] }))
        .await;
    
    assert_eq!(response.status(), 503);
}
```

### Implementation Steps
1. Add `Router` to `AppState`
2. Construct Router from config in main
3. In chat_completions handler, use `router.select_backend()`
4. Convert `RoutingError` to appropriate HTTP status codes:
   - ModelNotFound → 404
   - NoHealthyBackend → 503
   - CapabilityMismatch → 400
   - FallbackChainExhausted → 503
5. Add routing info to logs

### Acceptance Criteria
- [X] Router available in request handlers
- [X] Chat completions uses router for backend selection
- [X] ModelNotFound returns 404 with OpenAI error format
- [X] NoHealthyBackend returns 503
- [X] CapabilityMismatch returns 400
- [X] Routing decision logged at debug level

---

## T14: Add Integration Tests

**Objective**: End-to-end routing tests

### Tests to Write
```rust
// tests/routing_integration.rs

#[tokio::test]
async fn smart_routing_prefers_lower_load() {
    // Setup: Two backends, one with high load
    // Verify requests go to lower-load backend
}

#[tokio::test]
async fn alias_routing_works_e2e() {
    // Configure alias gpt-4 -> llama3:70b
    // Request gpt-4
    // Verify routed to backend with llama3:70b
}

#[tokio::test]
async fn fallback_chain_e2e() {
    // Configure fallback chain
    // Make primary unavailable
    // Verify routed to fallback
}

#[tokio::test]
async fn capability_filtering_e2e() {
    // Two backends, one with vision
    // Send vision request
    // Verify routed to vision-capable backend
}

#[tokio::test]
async fn concurrent_routing_decisions() {
    // Send 100 concurrent requests
    // Verify no errors, all routed correctly
}
```

### Implementation Steps
1. Create `tests/routing_integration.rs`
2. Setup helper functions for test app creation
3. Implement each integration test
4. Add stress test for concurrent routing

### Acceptance Criteria
- [X] All routing strategies tested E2E
- [X] Alias resolution tested E2E
- [X] Fallback chains tested E2E
- [X] Capability filtering tested E2E
- [X] Concurrent routing works without errors

---

## T15: Performance Validation

**Objective**: Verify routing meets < 1ms target

### Tests to Write
```rust
// src/routing/mod.rs or benches/routing.rs
#[test]
fn routing_decision_under_1ms() {
    let router = create_router_with_100_backends();
    let requirements = create_typical_requirements();
    
    let start = Instant::now();
    for _ in 0..1000 {
        router.select_backend(&requirements).unwrap();
    }
    let elapsed = start.elapsed();
    
    let avg_ns = elapsed.as_nanos() / 1000;
    let avg_us = avg_ns / 1000;
    
    println!("Average routing time: {}μs", avg_us);
    assert!(avg_us < 1000, "Routing should be < 1ms, was {}μs", avg_us);
}

#[test]
fn routing_with_1000_models() {
    // Test with large model index
    // Should still be < 1ms
}
```

### Implementation Steps
1. Create benchmark tests
2. Test with 10, 100, 1000 backends
3. Test with varying model counts
4. Profile if performance target not met
5. Document results

### Acceptance Criteria
- [X] Routing decision < 1ms with 100 backends
- [X] Routing decision < 1ms with 1000 models
- [X] No performance regression under concurrent load
- [X] Performance documented in code comments

---

## Summary

| Phase | Tasks | Focus |
|-------|-------|-------|
| Core Engine | T01-T04 | Module structure, requirements, filtering, errors |
| Scoring | T05-T06 | Scoring function, smart strategy |
| Strategies | T07-T09 | Round-robin, priority-only, random |
| Substitution | T10-T11 | Aliases, fallbacks |
| Integration | T12-T15 | Config, API, E2E tests, performance |

**Total Tasks**: 15  
**Estimated Time**: ~24 hours
