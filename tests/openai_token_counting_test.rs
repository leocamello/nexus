//! Unit tests for OpenAI token counting with tiktoken-rs (T021)
//!
//! These tests verify that OpenAIAgent::count_tokens() accurately counts
//! tokens using the tiktoken-rs library with o200k_base encoding.

#[cfg(test)]
mod tests {
    use nexus::agent::{openai::OpenAIAgent, InferenceAgent, TokenCount};
    use reqwest::Client;
    use std::sync::Arc;

    fn create_test_agent() -> OpenAIAgent {
        OpenAIAgent::new(
            "test-agent".to_string(),
            "Test OpenAI Agent".to_string(),
            "https://api.openai.com".to_string(),
            "test-key".to_string(),
            Arc::new(Client::new()),
        )
    }

    #[tokio::test]
    async fn test_count_tokens_simple_message() {
        let agent = create_test_agent();

        // Simple single-word message - InferenceAgent trait method
        let token_count = InferenceAgent::count_tokens(&agent, "gpt-4-turbo", "hello").await;
        match token_count {
            TokenCount::Exact(tokens) => {
                assert!(tokens > 0, "Token count should be positive");
                assert!(tokens < 10, "Single word should have few tokens");
            }
            TokenCount::Heuristic(_) => panic!("Expected Exact token count, got Heuristic"),
        }
    }

    #[tokio::test]
    async fn test_count_tokens_with_special_characters() {
        let agent = create_test_agent();

        // Message with special characters
        let token_count =
            InferenceAgent::count_tokens(&agent, "gpt-4-turbo", "Hello, world! 🌍").await;
        match token_count {
            TokenCount::Exact(tokens) => {
                assert!(tokens > 0, "Should handle special characters");
            }
            TokenCount::Heuristic(_) => panic!("Expected Exact token count"),
        }
    }

    #[tokio::test]
    async fn test_count_tokens_long_text() {
        let agent = create_test_agent();

        // Longer text should have proportionally more tokens
        let short_text = "Hello";
        let long_text = "Hello world, this is a longer message with more tokens";

        let short_count = InferenceAgent::count_tokens(&agent, "gpt-4-turbo", short_text).await;
        let long_count = InferenceAgent::count_tokens(&agent, "gpt-4-turbo", long_text).await;

        match (short_count, long_count) {
            (TokenCount::Exact(short), TokenCount::Exact(long)) => {
                assert!(
                    long > short * 2,
                    "Long text should have significantly more tokens: short={}, long={}",
                    short,
                    long
                );
            }
            _ => panic!("Expected Exact token counts"),
        }
    }

    #[tokio::test]
    async fn test_count_tokens_empty_string() {
        let agent = create_test_agent();

        let token_count = InferenceAgent::count_tokens(&agent, "gpt-4-turbo", "").await;
        match token_count {
            TokenCount::Exact(tokens) => {
                assert_eq!(tokens, 0, "Empty string should have 0 tokens");
            }
            TokenCount::Heuristic(_) => panic!("Expected Exact token count"),
        }
    }

    #[tokio::test]
    async fn test_count_tokens_code_snippet() {
        let agent = create_test_agent();

        // Code should be tokenized differently than prose
        let code = "fn main() { println!(\"Hello, world!\"); }";
        let token_count = InferenceAgent::count_tokens(&agent, "gpt-4-turbo", code).await;
        match token_count {
            TokenCount::Exact(tokens) => {
                assert!(tokens > 5, "Code should be tokenized into multiple tokens");
            }
            TokenCount::Heuristic(_) => panic!("Expected Exact token count"),
        }
    }
}
