//! CF-XCCDF v0.1 secure XML parsing and export.
//!
//! Uses `quick-xml` with DTD/entity/network processing disabled.
//! Parsed structures are typed Rust representations of XCCDF 1.2 and the
//! Crystal Forge extension namespace.

//! The database-free reading and analysis modules live in the `cf-compliance`
//! crate and are re-exported here unchanged, so existing
//! `crate::compliance::xccdf::*` paths keep resolving.

pub use cf_compliance::xccdf::{
    export_models, import_models, importer, inference, models, package, parser, reconciliation,
    xml_writer, zip_extractor,
};

mod exact_technical_match_db;

/// Exact technical matching: the pure identity derivation from `cf-compliance`
/// combined with the server-local database queries.
pub mod exact_technical_match {
    pub use cf_compliance::xccdf::exact_technical_match::*;

    pub use super::exact_technical_match_db::*;
}

pub mod disa_stig_adapter;
