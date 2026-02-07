//! Nexus - Distributed LLM model serving orchestrator
//!
//! This library provides the core functionality for managing and routing requests
//! to heterogeneous LLM inference backends.

pub mod api;
pub mod cli;
pub mod config;
pub mod discovery;
pub mod health;
pub mod registry;
