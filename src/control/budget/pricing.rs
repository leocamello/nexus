//! Model pricing registry for cost estimation

use std::collections::HashMap;

/// Pricing for input/output tokens
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ModelPricing {
    /// Price per 1M input tokens (USD)
    pub input_cost_per_million: f64,
    /// Price per 1M output tokens (USD)
    pub output_cost_per_million: f64,
}

impl ModelPricing {
    /// Zero-cost pricing for local models
    pub const LOCAL: ModelPricing = ModelPricing {
        input_cost_per_million: 0.0,
        output_cost_per_million: 0.0,
    };
}

/// Global pricing registry (hardcoded, updated with releases)
pub struct PricingRegistry {
    /// Map of model pattern to pricing
    pricing_map: HashMap<String, ModelPricing>,
}

impl PricingRegistry {
    /// Create registry with default pricing tables (Jan 2025 pricing)
    pub fn default_registry() -> Self {
        let mut pricing_map = HashMap::new();

        // OpenAI models (https://openai.com/api/pricing/)
        pricing_map.insert(
            "gpt-4o".to_string(),
            ModelPricing {
                input_cost_per_million: 2.50,
                output_cost_per_million: 10.00,
            },
        );
        pricing_map.insert(
            "gpt-4o-mini".to_string(),
            ModelPricing {
                input_cost_per_million: 0.15,
                output_cost_per_million: 0.60,
            },
        );
        pricing_map.insert(
            "gpt-4-turbo".to_string(),
            ModelPricing {
                input_cost_per_million: 10.00,
                output_cost_per_million: 30.00,
            },
        );
        pricing_map.insert(
            "gpt-4".to_string(),
            ModelPricing {
                input_cost_per_million: 30.00,
                output_cost_per_million: 60.00,
            },
        );
        pricing_map.insert(
            "gpt-3.5-turbo".to_string(),
            ModelPricing {
                input_cost_per_million: 0.50,
                output_cost_per_million: 1.50,
            },
        );

        // Anthropic models (https://www.anthropic.com/pricing)
        pricing_map.insert(
            "claude-3-opus".to_string(),
            ModelPricing {
                input_cost_per_million: 15.00,
                output_cost_per_million: 75.00,
            },
        );
        pricing_map.insert(
            "claude-3-sonnet".to_string(),
            ModelPricing {
                input_cost_per_million: 3.00,
                output_cost_per_million: 15.00,
            },
        );
        pricing_map.insert(
            "claude-3-haiku".to_string(),
            ModelPricing {
                input_cost_per_million: 0.25,
                output_cost_per_million: 1.25,
            },
        );
        pricing_map.insert(
            "claude-3.5-sonnet".to_string(),
            ModelPricing {
                input_cost_per_million: 3.00,
                output_cost_per_million: 15.00,
            },
        );

        // Local models (Ollama, LlamaCpp, vLLM on-premise)
        pricing_map.insert("llama".to_string(), ModelPricing::LOCAL);
        pricing_map.insert("mistral".to_string(), ModelPricing::LOCAL);
        pricing_map.insert("mixtral".to_string(), ModelPricing::LOCAL);
        pricing_map.insert("phi".to_string(), ModelPricing::LOCAL);
        pricing_map.insert("gemma".to_string(), ModelPricing::LOCAL);
        pricing_map.insert("qwen".to_string(), ModelPricing::LOCAL);

        // Fallback for unknown models (conservative: highest OpenAI GPT-4 tier)
        pricing_map.insert(
            "__unknown__".to_string(),
            ModelPricing {
                input_cost_per_million: 30.00,
                output_cost_per_million: 60.00,
            },
        );

        Self { pricing_map }
    }

    /// Get pricing for a model (with fallback to unknown)
    ///
    /// # Lookup strategy
    /// 1. Exact match (e.g., "gpt-4o")
    /// 2. Prefix match (e.g., "gpt-4o-2024-05-13" matches "gpt-4o")
    /// 3. Fallback to __unknown__ (conservative estimate)
    pub fn get_pricing(&self, model: &str) -> ModelPricing {
        // Normalize model name to lowercase for case-insensitive matching
        let model_lower = model.to_lowercase();

        // Try exact match first
        if let Some(pricing) = self.pricing_map.get(&model_lower) {
            return *pricing;
        }

        // Try prefix match, prioritize longer prefixes
        // Sort patterns by length descending to match most specific pattern first
        let mut patterns: Vec<(&String, &ModelPricing)> = self
            .pricing_map
            .iter()
            .filter(|(pattern, _)| *pattern != "__unknown__")
            .collect();
        patterns.sort_by_key(|(pattern, _)| std::cmp::Reverse(pattern.len()));

        for (pattern, pricing) in patterns {
            if model_lower.starts_with(pattern.as_str()) {
                return *pricing;
            }
        }

        // Fallback to unknown (conservative estimate with 1.15x multiplier applied elsewhere)
        self.pricing_map["__unknown__"]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pricing_registry_openai_exact() {
        let registry = PricingRegistry::default_registry();

        let pricing = registry.get_pricing("gpt-4o");
        assert_eq!(pricing.input_cost_per_million, 2.50);
        assert_eq!(pricing.output_cost_per_million, 10.00);

        let pricing = registry.get_pricing("gpt-4o-mini");
        assert_eq!(pricing.input_cost_per_million, 0.15);
        assert_eq!(pricing.output_cost_per_million, 0.60);
    }

    #[test]
    fn test_pricing_registry_anthropic_exact() {
        let registry = PricingRegistry::default_registry();

        let pricing = registry.get_pricing("claude-3-opus");
        assert_eq!(pricing.input_cost_per_million, 15.00);
        assert_eq!(pricing.output_cost_per_million, 75.00);

        let pricing = registry.get_pricing("claude-3-haiku");
        assert_eq!(pricing.input_cost_per_million, 0.25);
        assert_eq!(pricing.output_cost_per_million, 1.25);
    }

    #[test]
    fn test_pricing_registry_local_models() {
        let registry = PricingRegistry::default_registry();

        let pricing = registry.get_pricing("llama");
        assert_eq!(pricing.input_cost_per_million, 0.0);
        assert_eq!(pricing.output_cost_per_million, 0.0);

        let pricing = registry.get_pricing("mistral");
        assert_eq!(pricing.input_cost_per_million, 0.0);
        assert_eq!(pricing.output_cost_per_million, 0.0);
    }

    #[test]
    fn test_pricing_registry_prefix_match() {
        let registry = PricingRegistry::default_registry();

        // OpenAI versioned models
        let pricing = registry.get_pricing("gpt-4o-2024-05-13");
        assert_eq!(pricing.input_cost_per_million, 2.50);
        assert_eq!(pricing.output_cost_per_million, 10.00);

        // Anthropic versioned models
        let pricing = registry.get_pricing("claude-3-opus-20240229");
        assert_eq!(pricing.input_cost_per_million, 15.00);
        assert_eq!(pricing.output_cost_per_million, 75.00);

        // Local versioned models
        let pricing = registry.get_pricing("llama3:70b");
        assert_eq!(pricing.input_cost_per_million, 0.0);
        assert_eq!(pricing.output_cost_per_million, 0.0);
    }

    #[test]
    fn test_pricing_registry_unknown_fallback() {
        let registry = PricingRegistry::default_registry();

        // Completely unknown model
        let pricing = registry.get_pricing("unknown-model-xyz");
        assert_eq!(pricing.input_cost_per_million, 30.00);
        assert_eq!(pricing.output_cost_per_million, 60.00);
    }

    #[test]
    fn test_pricing_registry_case_insensitive() {
        let registry = PricingRegistry::default_registry();

        let pricing1 = registry.get_pricing("GPT-4O");
        let pricing2 = registry.get_pricing("gpt-4o");
        assert_eq!(
            pricing1.input_cost_per_million,
            pricing2.input_cost_per_million
        );
        assert_eq!(
            pricing1.output_cost_per_million,
            pricing2.output_cost_per_million
        );
    }

    #[test]
    fn test_model_pricing_local_constant() {
        assert_eq!(ModelPricing::LOCAL.input_cost_per_million, 0.0);
        assert_eq!(ModelPricing::LOCAL.output_cost_per_million, 0.0);
    }
}
