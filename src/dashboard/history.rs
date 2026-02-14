//! Request history ring buffer implementation
//!
//! Maintains a circular buffer of the last 100 completed requests for display in the dashboard.

use std::collections::VecDeque;
use std::sync::RwLock;

use crate::dashboard::types::HistoryEntry;

/// Ring buffer for storing request history (max 100 entries)
pub struct RequestHistory {
    entries: RwLock<VecDeque<HistoryEntry>>,
    capacity: usize,
}

impl RequestHistory {
    /// Creates a new RequestHistory with the specified capacity
    pub fn new() -> Self {
        Self {
            entries: RwLock::new(VecDeque::with_capacity(100)),
            capacity: 100,
        }
    }

    /// Adds a new entry to the history, evicting the oldest if at capacity
    /// Validates entry fields before adding:
    /// - Timestamp must not be in the future
    /// - Model name truncated to 256 chars
    /// - Error message truncated to 1024 chars
    pub fn push(&self, mut entry: HistoryEntry) {
        // Validate timestamp is not in the future
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        if entry.timestamp > now + 60 {
            // Allow 60 second clock skew
            entry.timestamp = now;
        }

        // Truncate model name to 256 chars
        if entry.model.len() > 256 {
            entry.model.truncate(256);
        }

        // Truncate error message to 1024 chars
        if let Some(ref mut error_msg) = entry.error_message {
            if error_msg.len() > 1024 {
                error_msg.truncate(1024);
            }
        }

        let mut entries = self.entries.write().unwrap();
        if entries.len() >= self.capacity {
            entries.pop_front();
        }
        entries.push_back(entry);
    }

    /// Returns all entries in chronological order (oldest first)
    pub fn get_all(&self) -> Vec<HistoryEntry> {
        self.entries.read().unwrap().iter().cloned().collect()
    }

    /// Returns the number of entries currently stored
    pub fn len(&self) -> usize {
        self.entries.read().unwrap().len()
    }

    /// Returns true if the history is empty
    pub fn is_empty(&self) -> bool {
        self.entries.read().unwrap().is_empty()
    }
}

impl Default for RequestHistory {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dashboard::types::RequestStatus;

    #[test]
    fn test_new_creates_empty_history() {
        let history = RequestHistory::new();
        assert_eq!(history.len(), 0);
        assert!(history.is_empty());
    }

    #[test]
    fn test_push_adds_entry() {
        let history = RequestHistory::new();
        let entry = HistoryEntry {
            timestamp: 1234567890,
            model: "gpt-4".to_string(),
            backend_id: "backend-1".to_string(),
            duration_ms: 150,
            status: RequestStatus::Success,
            error_message: None,
        };

        history.push(entry.clone());
        assert_eq!(history.len(), 1);

        let entries = history.get_all();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].model, "gpt-4");
    }

    #[test]
    fn test_ring_buffer_eviction_fifo() {
        let history = RequestHistory::new();

        // Add 105 entries to exceed capacity of 100
        for i in 0..105 {
            let entry = HistoryEntry {
                timestamp: i as u64,
                model: format!("model-{}", i),
                backend_id: format!("backend-{}", i % 3),
                duration_ms: 100 + i as u64,
                status: RequestStatus::Success,
                error_message: None,
            };
            history.push(entry);
        }

        // Should have exactly 100 entries
        assert_eq!(history.len(), 100);

        // First entry should be the 6th pushed (0-4 were evicted)
        let entries = history.get_all();
        assert_eq!(entries[0].timestamp, 5);
        assert_eq!(entries[0].model, "model-5");

        // Last entry should be the 105th pushed
        assert_eq!(entries[99].timestamp, 104);
        assert_eq!(entries[99].model, "model-104");
    }

    #[test]
    fn test_get_all_returns_chronological_order() {
        let history = RequestHistory::new();

        // Add entries with different timestamps
        for i in (0..5).rev() {
            let entry = HistoryEntry {
                timestamp: i as u64,
                model: format!("model-{}", i),
                backend_id: "backend-1".to_string(),
                duration_ms: 100,
                status: RequestStatus::Success,
                error_message: None,
            };
            history.push(entry);
        }

        let entries = history.get_all();
        // Even though we added in reverse, get_all should return in push order
        assert_eq!(entries.len(), 5);
        assert_eq!(entries[0].timestamp, 4);
        assert_eq!(entries[4].timestamp, 0);
    }
}
