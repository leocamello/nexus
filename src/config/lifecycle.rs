//! Lifecycle management configuration

use serde::{Deserialize, Serialize};

/// Configuration for model lifecycle operations (load/unload/migrate).
///
/// Controls timeouts, VRAM management, and safety thresholds for lifecycle operations.
///
/// # Example
///
/// ```toml
/// [lifecycle]
/// timeout_ms = 300000
/// vram_headroom_percent = 20
/// vram_buffer_percent = 10
/// vram_heuristic_max_gb = 16
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct LifecycleConfig {
    /// Maximum time to wait for a model load/unload operation in milliseconds.
    ///
    /// Default: 300000ms (5 minutes)
    /// Operations exceeding this timeout will be marked as failed.
    pub timeout_ms: u64,

    /// VRAM headroom percentage to keep free when loading models.
    ///
    /// Default: 20 (20% of total VRAM)
    /// Prevents OOM by refusing loads if free VRAM would drop below this threshold.
    pub vram_headroom_percent: u8,

    /// Additional VRAM buffer percentage for safety margin.
    ///
    /// Default: 10 (10% of total VRAM)
    /// Added buffer on top of headroom for unexpected VRAM spikes.
    pub vram_buffer_percent: u8,

    /// VRAM heuristic maximum threshold in GB.
    ///
    /// Default: 16 (16GB)
    /// When the backend does not report total VRAM (e.g., Ollama), load requests
    /// are rejected if used VRAM exceeds this threshold. Adjust to match your GPU.
    pub vram_heuristic_max_gb: u64,
}

impl Default for LifecycleConfig {
    fn default() -> Self {
        Self {
            timeout_ms: 300_000, // 5 minutes
            vram_headroom_percent: 20,
            vram_buffer_percent: 10,
            vram_heuristic_max_gb: 16,
        }
    }
}
