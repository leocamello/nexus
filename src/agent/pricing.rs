//! Cost estimation for cloud LLM providers.
//!
//! This module provides token-based cost estimation for cloud inference backends
//! (OpenAI, Anthropic, Google AI). Pricing data is hardcoded and must be manually
//! updated when providers change their pricing.
//!
//! ## Pricing Strategy
//!
//! - **Input tokens**: Charged at per-1K-token rate for prompt/context
//! - **Output tokens**: Charged at per-1K-token rate for completion
//! - **Total cost**: `(input_tokens/1000 * input_rate) + (output_tokens/1000 * output_rate)`
//!
//! ## Maintenance
//!
//! Pricing must be manually updated. Check provider pricing pages quarterly:
//! - OpenAI: https://openai.com/pricing
//! - Anthropic: https://www.anthropic.com/pricing
//! - Google AI: https://ai.google.dev/pricing
//!
//! ## Example
//!
//! ```rust
//! use nexus::agent::pricing::PricingTable;
//!
//! let pricing = PricingTable::new();
//! let cost = pricing.estimate_cost("gpt-4-turbo", 1000, 500);
//! assert_eq!(cost, Some(0.025)); // $0.01/1K input + $0.03/1K output = $0.025
//! ```

use std::collections::HashMap;
use std::sync::Arc;

/// Pricing for a specific model (input and output rates).
#[derive(Debug, Clone, Copy)]
pub struct ModelPricing {
    /// Input (prompt) cost in USD per 1K tokens.
    pub input_price_per_1k: f64,

    /// Output (completion) cost in USD per 1K tokens.
    pub output_price_per_1k: f64,
}

/// Pricing table for all supported cloud models.
///
/// This table is initialized once at startup and shared across all agents.
/// Prices are current as of February 2024.
#[derive(Debug, Clone)]
pub struct PricingTable {
    prices: Arc<HashMap<String, ModelPricing>>,
}

impl PricingTable {
    /// Create a new pricing table with current rates (Feb 2024).
    pub fn new() -> Self {
        let mut prices = HashMap::new();

        // OpenAI Pricing (https://openai.com/pricing)
        prices.insert(
            "gpt-4-turbo".to_string(),
            ModelPricing {
                input_price_per_1k: 0.01,
                output_price_per_1k: 0.03,
            },
        );
        prices.insert(
            "gpt-4-turbo-preview".to_string(),
            ModelPricing {
                input_price_per_1k: 0.01,
                output_price_per_1k: 0.03,
            },
        );
        prices.insert(
            "gpt-4".to_string(),
            ModelPricing {
                input_price_per_1k: 0.03,
                output_price_per_1k: 0.06,
            },
        );
        prices.insert(
            "gpt-3.5-turbo".to_string(),
            ModelPricing {
                input_price_per_1k: 0.0005,
                output_price_per_1k: 0.0015,
            },
        );

        // Anthropic Pricing (https://www.anthropic.com/pricing)
        prices.insert(
            "claude-3-opus-20240229".to_string(),
            ModelPricing {
                input_price_per_1k: 0.015,
                output_price_per_1k: 0.075,
            },
        );
        prices.insert(
            "claude-3-sonnet-20240229".to_string(),
            ModelPricing {
                input_price_per_1k: 0.003,
                output_price_per_1k: 0.015,
            },
        );
        prices.insert(
            "claude-3-haiku-20240307".to_string(),
            ModelPricing {
                input_price_per_1k: 0.00025,
                output_price_per_1k: 0.00125,
            },
        );

        // Google AI Pricing (https://ai.google.dev/pricing)
        prices.insert(
            "gemini-1.5-pro".to_string(),
            ModelPricing {
                input_price_per_1k: 0.0035,
                output_price_per_1k: 0.0105,
            },
        );
        prices.insert(
            "gemini-1.5-flash".to_string(),
            ModelPricing {
                input_price_per_1k: 0.00035,
                output_price_per_1k: 0.00105,
            },
        );
        prices.insert(
            "gemini-1.0-pro".to_string(),
            ModelPricing {
                input_price_per_1k: 0.0005,
                output_price_per_1k: 0.0015,
            },
        );

        Self {
            prices: Arc::new(prices),
        }
    }

    /// Estimate cost for a model based on token counts.
    ///
    /// Returns `None` if the model is not in the pricing table.
    ///
    /// ## Arguments
    ///
    /// - `model`: Model ID (e.g., "gpt-4-turbo", "claude-3-opus-20240229")
    /// - `input_tokens`: Number of input (prompt) tokens
    /// - `output_tokens`: Number of output (completion) tokens
    ///
    /// ## Returns
    ///
    /// Estimated cost in USD, or `None` if model pricing is unknown.
    pub fn estimate_cost(&self, model: &str, input_tokens: u32, output_tokens: u32) -> Option<f64> {
        self.prices.get(model).map(|pricing| {
            let input_cost = (input_tokens as f64 / 1000.0) * pricing.input_price_per_1k;
            let output_cost = (output_tokens as f64 / 1000.0) * pricing.output_price_per_1k;
            input_cost + output_cost
        })
    }

    /// Check if a model has pricing data available.
    pub fn has_pricing(&self, model: &str) -> bool {
        self.prices.contains_key(model)
    }

    /// Get pricing details for a model.
    pub fn get_pricing(&self, model: &str) -> Option<ModelPricing> {
        self.prices.get(model).copied()
    }
}

impl Default for PricingTable {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_openai_pricing() {
        let pricing = PricingTable::new();

        // GPT-4 Turbo: $0.01 input + $0.03 output per 1K tokens
        let cost = pricing.estimate_cost("gpt-4-turbo", 1000, 500);
        assert_eq!(cost, Some(0.025)); // (1000/1000)*0.01 + (500/1000)*0.03 = 0.01 + 0.015 = 0.025

        // GPT-3.5 Turbo: $0.0005 input + $0.0015 output per 1K tokens
        let cost = pricing.estimate_cost("gpt-3.5-turbo", 2000, 1000);
        assert_eq!(cost, Some(0.0025)); // (2000/1000)*0.0005 + (1000/1000)*0.0015 = 0.001 + 0.0015 = 0.0025
    }

    #[test]
    fn test_anthropic_pricing() {
        let pricing = PricingTable::new();

        // Claude 3 Opus: $0.015 input + $0.075 output per 1K tokens
        let cost = pricing.estimate_cost("claude-3-opus-20240229", 1000, 500);
        assert_eq!(cost, Some(0.0525)); // 0.015 + 0.0375 = 0.0525
    }

    #[test]
    fn test_unknown_model() {
        let pricing = PricingTable::new();
        let cost = pricing.estimate_cost("unknown-model", 1000, 500);
        assert_eq!(cost, None);
    }

    #[test]
    fn test_has_pricing() {
        let pricing = PricingTable::new();
        assert!(pricing.has_pricing("gpt-4-turbo"));
        assert!(pricing.has_pricing("claude-3-opus-20240229"));
        assert!(pricing.has_pricing("gemini-1.5-pro"));
        assert!(!pricing.has_pricing("unknown-model"));
    }
}
