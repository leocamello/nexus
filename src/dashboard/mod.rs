//! Web dashboard module for real-time monitoring
//!
//! Provides an embedded web dashboard served at `/` that displays:
//! - Backend health status with real-time updates
//! - Model availability matrix
//! - Recent request history (last 100 requests)
//!
//! Uses WebSocket for real-time updates with automatic fallback to polling.

pub mod handler;
pub mod history;
pub mod types;
pub mod websocket;

pub use handler::{assets_handler, dashboard_handler, history_handler};
pub use websocket::websocket_handler;
