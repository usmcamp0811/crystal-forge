//! Custom hooks for Crystal Forge UI.

pub mod infinite_scroll;
pub mod websocket;

pub use infinite_scroll::{InfiniteScroll, use_infinite_scroll};
pub use websocket::{SystemMetrics, use_websocket_logs};
