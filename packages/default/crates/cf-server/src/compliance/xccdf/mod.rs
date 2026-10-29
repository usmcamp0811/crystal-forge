//! CF-XCCDF v0.1 secure XML parsing and export.
//!
//! Uses `quick-xml` with DTD/entity/network processing disabled.
//! Parsed structures are typed Rust representations of XCCDF 1.2 and the
//! Crystal Forge extension namespace.

//! The database-free reading and analysis modules live in the `cf-compliance`
//! crate and are re-exported here unchanged, so existing
//! `crate::compliance::xccdf::*` paths keep resolving.

pub use cf_compliance::xccdf::{
    custom_check, export_models, import_models, inference, models, package, parser, reconciliation,
    xml_writer, zip_extractor,
};

/// Import planning from `cf-compliance`, plus the server's async entry point.
///
/// The async wrapper lives here, not in `cf-compliance`, because that crate
/// depends on no async runtime.
pub mod importer {
    use cf_compliance::xccdf::import_models::{
        ImportPlanError, ImportedPolicyRecord, ValidatedImportPlan,
    };

    pub use cf_compliance::xccdf::importer::*;

    /// Validates a CF-native document without blocking the async executor.
    ///
    /// The synchronous validator remains available for offline callers and
    /// tests. Production async import paths must use this wrapper because
    /// composite policy validation can invoke the bounded Nix parser.
    ///
    /// # Errors
    ///
    /// Returns the synchronous validator's error. Returns
    /// `CF_NATIVE_PAYLOAD_INVALID` if Tokio cannot join the blocking validation
    /// task, including after a panic.
    pub async fn validate_cf_native_document_async(
        parsed: &cf_compliance::xccdf::models::ParsedXccdf,
    ) -> Result<(ValidatedImportPlan, Vec<ImportedPolicyRecord>), ImportPlanError> {
        let parsed = parsed.clone();
        tokio::task::spawn_blocking(move || validate_cf_native_document(&parsed))
            .await
            .map_err(|error| {
                ImportPlanError::cf_native_invalid(
                    "CF_NATIVE_PAYLOAD_INVALID",
                    format!("CF-native validation task failed: {error}"),
                )
            })?
    }
}

mod exact_technical_match_db;

/// Exact technical matching: the pure identity derivation from `cf-compliance`
/// combined with the server-local database queries.
pub mod exact_technical_match {
    pub use cf_compliance::xccdf::exact_technical_match::*;

    pub use super::exact_technical_match_db::*;
}

pub mod disa_stig_adapter;
