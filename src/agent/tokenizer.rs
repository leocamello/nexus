//! Token counting infrastructure for accurate cost estimation
//!
//! This module provides provider-specific tokenizers for audit-grade token counting
//! across OpenAI, Anthropic, and other LLM providers. It supports three tiers:
//! - **Exact**: Uses provider's official tokenizer (e.g., tiktoken for OpenAI)
//! - **Approximation**: Uses similar tokenizer as proxy (e.g., cl100k_base for Anthropic)
//! - **Heuristic**: Falls back to conservative character-based estimation
//!
//! # Example
//!
//! ```rust
//! use nexus::agent::tokenizer::TokenizerRegistry;
//!
//! let registry = TokenizerRegistry::new()?;
//! let token_count = registry.count_tokens("gpt-4-turbo", "Hello world")?;
//! ```

use globset::{Glob, GlobMatcher};
use std::sync::Arc;
use thiserror::Error;
use tiktoken_rs::CoreBPE;

// Tier constants for cost estimate accuracy
pub const TIER_EXACT: u8 = 0; // Exact match with provider's tokenizer
pub const TIER_APPROXIMATION: u8 = 1; // Similar tokenizer as approximation
pub const TIER_HEURISTIC: u8 = 2; // Character-based fallback

/// Errors that can occur during tokenization
#[derive(Debug, Error)]
pub enum TokenizerError {
    /// Failed to encode text
    #[error("Tokenization failed: {0}")]
    Encoding(String),

    /// Model not supported by this tokenizer
    #[error("Model not supported by tokenizer: {0}")]
    ModelNotSupported(String),

    /// Failed to compile glob pattern
    #[error("Invalid glob pattern: {0}")]
    GlobPattern(#[from] globset::Error),
}

/// Trait for token counting implementations
///
/// All tokenizers must be thread-safe (Send + Sync) as they're shared across
/// concurrent request processing.
pub trait Tokenizer: Send + Sync {
    /// Count tokens in the provided text
    fn count_tokens(&self, text: &str) -> Result<u32, TokenizerError>;

    /// Get the accuracy tier for this tokenizer
    /// - 0 = exact (provider's official tokenizer)
    /// - 1 = approximation (similar tokenizer)
    /// - 2 = heuristic (character-based estimation)
    fn tier(&self) -> u8;

    /// Human-readable name for logging and debugging
    fn name(&self) -> &str;
}

/// Exact tokenizer for OpenAI models using tiktoken
pub struct TiktokenExactTokenizer {
    encoding: CoreBPE,
    tier: u8,
    name: &'static str,
}

impl TiktokenExactTokenizer {
    /// Create tokenizer for GPT-4 Turbo and GPT-4o models using o200k_base
    pub fn o200k_base() -> Result<Self, TokenizerError> {
        Ok(Self {
            encoding: tiktoken_rs::o200k_base()
                .map_err(|e| TokenizerError::Encoding(format!("o200k_base: {}", e)))?,
            tier: TIER_EXACT,
            name: "tiktoken_o200k_base",
        })
    }

    /// Create tokenizer for GPT-3.5 and GPT-4 base models using cl100k_base
    pub fn cl100k_base() -> Result<Self, TokenizerError> {
        Ok(Self {
            encoding: tiktoken_rs::cl100k_base()
                .map_err(|e| TokenizerError::Encoding(format!("cl100k_base: {}", e)))?,
            tier: TIER_EXACT,
            name: "tiktoken_cl100k_base",
        })
    }
}

impl Tokenizer for TiktokenExactTokenizer {
    fn count_tokens(&self, text: &str) -> Result<u32, TokenizerError> {
        self.encoding
            .encode_with_special_tokens(text)
            .len()
            .try_into()
            .map_err(|e| TokenizerError::Encoding(format!("Token count overflow: {}", e)))
    }

    fn tier(&self) -> u8 {
        self.tier
    }

    fn name(&self) -> &str {
        self.name
    }
}

/// Approximation tokenizer using cl100k_base for non-OpenAI models
pub struct TiktokenApproximationTokenizer {
    encoding: CoreBPE,
}

impl TiktokenApproximationTokenizer {
    /// Create approximation tokenizer (used for Anthropic Claude)
    pub fn new() -> Result<Self, TokenizerError> {
        Ok(Self {
            encoding: tiktoken_rs::cl100k_base()
                .map_err(|e| TokenizerError::Encoding(format!("cl100k_base: {}", e)))?,
        })
    }
}

impl Tokenizer for TiktokenApproximationTokenizer {
    fn count_tokens(&self, text: &str) -> Result<u32, TokenizerError> {
        self.encoding
            .encode_with_special_tokens(text)
            .len()
            .try_into()
            .map_err(|e| TokenizerError::Encoding(format!("Token count overflow: {}", e)))
    }

    fn tier(&self) -> u8 {
        TIER_APPROXIMATION
    }

    fn name(&self) -> &str {
        "tiktoken_approximation"
    }
}

/// Heuristic tokenizer using character-based estimation
pub struct HeuristicTokenizer {
    multiplier: f64, // Conservative multiplier for character-based estimation
}

impl HeuristicTokenizer {
    /// Create heuristic tokenizer with 1.15x conservative multiplier
    pub fn new() -> Self {
        Self { multiplier: 1.15 }
    }
}

impl Tokenizer for HeuristicTokenizer {
    fn count_tokens(&self, text: &str) -> Result<u32, TokenizerError> {
        // Character-based heuristic: ~4 chars per token (English average)
        // Apply 1.15x multiplier for conservative estimate
        let base_estimate = (text.len() / 4).max(1);
        let conservative = (base_estimate as f64 * self.multiplier) as u32;
        Ok(conservative)
    }

    fn tier(&self) -> u8 {
        TIER_HEURISTIC
    }

    fn name(&self) -> &str {
        "heuristic"
    }
}

/// Registry for selecting appropriate tokenizer based on model name
pub struct TokenizerRegistry {
    /// Ordered list of (pattern, tokenizer) for matching models
    matchers: Vec<(GlobMatcher, Arc<dyn Tokenizer>)>,

    /// Fallback for unknown models
    fallback: Arc<dyn Tokenizer>,
}

impl TokenizerRegistry {
    /// Create registry with default OpenAI/Anthropic/fallback configuration
    pub fn new() -> Result<Self, TokenizerError> {
        let mut matchers = Vec::new();

        // OpenAI GPT-4 Turbo, GPT-4o → o200k_base (exact)
        let o200k_patterns = vec!["gpt-4-turbo*", "gpt-4o*"];
        for pattern in o200k_patterns {
            let glob = Glob::new(pattern)?;
            matchers.push((
                glob.compile_matcher(),
                Arc::new(TiktokenExactTokenizer::o200k_base()?) as Arc<dyn Tokenizer>,
            ));
        }

        // OpenAI GPT-3.5, GPT-4 base → cl100k_base (exact)
        let cl100k_patterns = vec!["gpt-3.5*", "gpt-4", "gpt-4-*"];
        for pattern in cl100k_patterns {
            let glob = Glob::new(pattern)?;
            matchers.push((
                glob.compile_matcher(),
                Arc::new(TiktokenExactTokenizer::cl100k_base()?),
            ));
        }

        // Anthropic Claude → cl100k_base (approximation)
        let claude_glob = Glob::new("claude-*")?;
        matchers.push((
            claude_glob.compile_matcher(),
            Arc::new(TiktokenApproximationTokenizer::new()?),
        ));

        // Fallback for all other models
        let fallback = Arc::new(HeuristicTokenizer::new());

        Ok(Self { matchers, fallback })
    }

    /// Find tokenizer for a model name
    pub fn get_tokenizer(&self, model: &str) -> Arc<dyn Tokenizer> {
        for (matcher, tokenizer) in &self.matchers {
            if matcher.is_match(model) {
                return Arc::clone(tokenizer);
            }
        }
        Arc::clone(&self.fallback)
    }

    /// Count tokens for a model + text (convenience method)
    /// 
    /// Records timing metrics and tier counters for observability (US2: Precise Tracking)
    pub fn count_tokens(&self, model: &str, text: &str) -> Result<u32, TokenizerError> {
        let tokenizer = self.get_tokenizer(model);
        let tier_name = Self::tier_name(tokenizer.tier());
        let model_name = model.to_string();
        
        // Measure tokenization duration (T022)
        let start = std::time::Instant::now();
        let result = tokenizer.count_tokens(text);
        let duration = start.elapsed();

        // Record metrics (T020, T021)
        metrics::histogram!(
            "nexus_token_count_duration_seconds",
            "tier" => tier_name,
            "model" => model_name.clone()
        ).record(duration.as_secs_f64());

        metrics::counter!(
            "nexus_token_count_tier_total",
            "tier" => tier_name,
            "model" => model_name
        ).increment(1);

        result
    }

    /// Get tier name as string for metrics
    pub fn tier_name(tier: u8) -> &'static str {
        match tier {
            TIER_EXACT => "exact",
            TIER_APPROXIMATION => "approximation",
            TIER_HEURISTIC => "heuristic",
            _ => "unknown",
        }
    }
}
