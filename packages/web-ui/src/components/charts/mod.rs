//! Chart components for data visualization.
//!
//! Provides reusable chart components like donut charts, pie charts,
//! and their associated data types and helper functions.

mod donut;

pub use donut::{DonutArc, DonutChartWithLegend, DonutSegment};
