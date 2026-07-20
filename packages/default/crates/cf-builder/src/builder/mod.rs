//! Builder API client and related utilities for the remote build worker.
//!
//! This module contains the client-side builder components:
//! - `api_client`: HTTP/WebSocket client for builder↔server communication
//! - `metrics`: System metrics collection (CPU, memory)
//! - `status`: Worker status tracking
//! - `error`: Error types

pub mod api_client;
pub mod error;
pub mod metrics;
pub mod status;

pub use api_client::{ApiBuildReporter, BuilderApiClient};
pub use metrics::SystemMetrics;
