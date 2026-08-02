//! CF-XCCDF v0.1 secure XML parsing and export.
//!
//! Uses `quick-xml` with DTD/entity/network processing disabled.
//! Parsed structures are typed Rust representations of XCCDF 1.2 and the
//! Crystal Forge extension namespace.

pub mod models;
pub mod parser;
pub mod xml_writer;
pub mod zip_extractor;
