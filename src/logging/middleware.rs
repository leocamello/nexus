//! Request ID generation middleware

use uuid::Uuid;

/// Generate a new request ID using UUID v4
///
/// Returns a unique correlation ID that can be used to track
/// requests through the system, including retries and fallbacks.
///
/// # Examples
///
/// ```
/// use nexus::logging::generate_request_id;
///
/// let request_id = generate_request_id();
/// assert!(!request_id.is_empty());
/// ```
pub fn generate_request_id() -> String {
    Uuid::new_v4().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_request_id_format() {
        let id = generate_request_id();
        // UUID v4 format: xxxxxxxx-xxxx-4xxx-yxxx-xxxxxxxxxxxx
        assert_eq!(id.len(), 36);
        assert_eq!(id.chars().filter(|&c| c == '-').count(), 4);
    }

    #[test]
    fn test_generate_request_id_uniqueness() {
        let id1 = generate_request_id();
        let id2 = generate_request_id();
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_generate_request_id_parseable() {
        let id = generate_request_id();
        let parsed = Uuid::parse_str(&id);
        assert!(parsed.is_ok());
    }
}
