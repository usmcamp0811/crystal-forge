//! CF-XCCDF v0.1 secure XML reading and pure interchange analysis.
//!
//! Uses `quick-xml` with DTD/entity/network processing disabled.
//! Parsed structures are typed Rust representations of XCCDF 1.2 and the
//! Crystal Forge extension namespace.
//!
//! Only database-free modules live here. Persistence-bound XCCDF code
//! (`disa_stig_adapter` and the database queries behind
//! `exact_technical_match`) remains in `cf-server`, as does the async
//! `validate_cf_native_document_async` wrapper, because this crate depends on
//! no async runtime.

pub mod custom_check;
pub mod exact_technical_match;
pub mod export_models;
pub mod import_models;
pub mod importer;
pub mod inference;
pub mod models;
pub mod package;
pub mod parser;
pub mod reconciliation;
pub mod xml_writer;
pub mod zip_extractor;
