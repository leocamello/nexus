//! Unit tests for PricingTable cost estimation (T022)
//!
//! Verifies that cost estimation works correctly with known token counts
//! across different models and providers.

#[cfg(test)]
mod tests {
    use nexus::agent::pricing::PricingTable;

    #[test]
    fn test_estimate_cost_gpt4_turbo() {
        let pricing = PricingTable::new();

        // GPT-4 Turbo: $0.01 per 1K input tokens, $0.03 per 1K output tokens
        // 1000 input + 500 output = (1000/1000)*0.01 + (500/1000)*0.03 = 0.01 + 0.015 = 0.025
        let cost = pricing.estimate_cost("gpt-4-turbo", 1000, 500);
        assert_eq!(cost, Some(0.025));
    }

    #[test]
    fn test_estimate_cost_gpt35_turbo() {
        let pricing = PricingTable::new();

        // GPT-3.5 Turbo: $0.0005 per 1K input, $0.0015 per 1K output
        // 2000 input + 1000 output = (2000/1000)*0.0005 + (1000/1000)*0.0015 = 0.001 + 0.0015 = 0.0025
        let cost = pricing.estimate_cost("gpt-3.5-turbo", 2000, 1000);
        assert_eq!(cost, Some(0.0025));
    }

    #[test]
    fn test_estimate_cost_claude_opus() {
        let pricing = PricingTable::new();

        // Claude 3 Opus: $0.015 per 1K input, $0.075 per 1K output
        // 1000 input + 500 output = 0.015 + 0.0375 = 0.0525
        let cost = pricing.estimate_cost("claude-3-opus-20240229", 1000, 500);
        assert_eq!(cost, Some(0.0525));
    }

    #[test]
    fn test_estimate_cost_gemini_pro() {
        let pricing = PricingTable::new();

        // Gemini 1.5 Pro: $0.0035 per 1K input, $0.0105 per 1K output
        // 1000 input + 1000 output = 0.0035 + 0.0105 = 0.014
        let cost = pricing.estimate_cost("gemini-1.5-pro", 1000, 1000);
        assert_eq!(cost, Some(0.014));
    }

    #[test]
    fn test_estimate_cost_unknown_model() {
        let pricing = PricingTable::new();

        // Unknown model should return None
        let cost = pricing.estimate_cost("unknown-model-xyz", 1000, 500);
        assert_eq!(cost, None);
    }

    #[test]
    fn test_estimate_cost_zero_tokens() {
        let pricing = PricingTable::new();

        // Zero tokens should result in zero cost
        let cost = pricing.estimate_cost("gpt-4-turbo", 0, 0);
        assert_eq!(cost, Some(0.0));
    }

    #[test]
    fn test_estimate_cost_large_token_counts() {
        let pricing = PricingTable::new();

        // Large token counts (1M input, 500K output)
        // GPT-4 Turbo: (1_000_000/1000)*0.01 + (500_000/1000)*0.03 = 10.0 + 15.0 = 25.0
        let cost = pricing.estimate_cost("gpt-4-turbo", 1_000_000, 500_000);
        assert_eq!(cost, Some(25.0));
    }

    #[test]
    fn test_estimate_cost_fractional_tokens() {
        let pricing = PricingTable::new();

        // Small token counts that result in fractional costs
        // GPT-3.5 Turbo: (100/1000)*0.0005 + (100/1000)*0.0015 = 0.00005 + 0.00015 = 0.0002
        let cost = pricing.estimate_cost("gpt-3.5-turbo", 100, 100);
        assert_eq!(cost, Some(0.0002));
    }

    #[test]
    fn test_has_pricing() {
        let pricing = PricingTable::new();

        assert!(pricing.has_pricing("gpt-4-turbo"));
        assert!(pricing.has_pricing("gpt-3.5-turbo"));
        assert!(pricing.has_pricing("claude-3-opus-20240229"));
        assert!(pricing.has_pricing("gemini-1.5-pro"));
        assert!(!pricing.has_pricing("unknown-model"));
    }
}
