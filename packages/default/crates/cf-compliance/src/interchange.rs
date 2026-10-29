//! Stable CF-XCCDF v0.1 identifiers and bounded interchange input limits.

use std::fmt;

/// XCCDF 1.1.4 namespace, used by older DISA STIGs and similar documents.
pub const XCCDF_1_1_NAMESPACE: &str = "http://checklists.nist.gov/xccdf/1.1";
/// XCCDF 1.2 namespace — Crystal Forge's canonical export namespace.
pub const XCCDF_1_2_NAMESPACE: &str = "http://checklists.nist.gov/xccdf/1.2";
/// Alias for the CF export namespace. Import accepts both 1.1 and 1.2.
pub const XCCDF_NAMESPACE: &str = XCCDF_1_2_NAMESPACE;
/// Crystal Forge's versioned XCCDF extension namespace.
pub const CF_XCCDF_NAMESPACE: &str = "urn:crystal-forge:xccdf:1";
/// XCCDF check system for Crystal Forge policy implementations.
pub const CF_POLICY_CHECK_SYSTEM: &str = "urn:crystal-forge:check-system:policy:1";
/// XCCDF fix system for Crystal Forge Nix remediation content.
pub const CF_NIX_FIX_SYSTEM: &str = "urn:crystal-forge:fix-system:nix:1";
/// Stable semantic canonicalization format used for interchange digests.
pub const CANONICALIZATION_VERSION: &str = "cf-model-json-1";
/// Digest algorithm used with [`CANONICALIZATION_VERSION`].
pub const DIGEST_ALGORITHM: &str = "sha-256";

/// Maximum accepted plain XML upload size (file-field bytes).
pub const MAX_XCCDF_XML_BYTES: usize = 10 * 1024 * 1024;
/// Maximum accepted ZIP package upload size (file-field bytes).
///
/// DISA STIGs are frequently packaged as multi-megabyte ZIPs containing PDFs,
/// stylesheets, and the XCCDF XML.  50 MiB covers current and near-future
/// packages while remaining a practical denial-of-service bound.
pub const MAX_XCCDF_ZIP_BYTES: usize = 50 * 1024 * 1024;
/// Largest allowed upload regardless of type — the accumulation limit and
/// route body limit are both set from this value.
pub const MAX_XCCDF_UPLOAD_BYTES: usize = if MAX_XCCDF_ZIP_BYTES > MAX_XCCDF_XML_BYTES {
    MAX_XCCDF_ZIP_BYTES
} else {
    MAX_XCCDF_XML_BYTES
};
/// Allowance for multipart framing (boundaries, headers) around the file field.
pub const MAX_MULTIPART_OVERHEAD_BYTES: usize = 64 * 1024;
/// Maximum accepted multipart request size for XCCDF upload routes.
pub const MAX_XCCDF_MULTIPART_BYTES: usize = MAX_XCCDF_UPLOAD_BYTES + MAX_MULTIPART_OVERHEAD_BYTES;

/// Limits applied before XML or archive contents are parsed.
///
/// The limits deliberately bound both the compressed transport and the parsed
/// representation. Future import endpoints must use these values rather than
/// accepting request-size defaults as their only resource control.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InterchangeLimits {
    pub max_xml_bytes: usize,
    pub max_zip_bytes: usize,
    pub max_expanded_archive_bytes: usize,
    pub max_archive_files: usize,
    pub max_xml_depth: usize,
    pub max_attributes_per_element: usize,
    pub max_text_node_bytes: usize,
    pub max_rule_count: usize,
    pub max_profile_count: usize,
    pub max_group_count: usize,
    pub max_value_count: usize,
    pub max_policy_expression_bytes: usize,
    pub max_preserved_opaque_xml_bytes: usize,
}

impl Default for InterchangeLimits {
    fn default() -> Self {
        Self {
            max_xml_bytes: MAX_XCCDF_XML_BYTES,
            max_zip_bytes: 50 * 1024 * 1024,
            max_expanded_archive_bytes: 100 * 1024 * 1024,
            max_archive_files: 1_000,
            max_xml_depth: 64,
            max_attributes_per_element: 128,
            max_text_node_bytes: 1024 * 1024,
            max_rule_count: 5_000,
            max_profile_count: 100,
            max_group_count: 1_000,
            max_value_count: 5_000,
            max_policy_expression_bytes: 128 * 1024,
            max_preserved_opaque_xml_bytes: 512 * 1024,
        }
    }
}

impl InterchangeLimits {
    /// Reject an XML document whose transport size exceeds the configured bound.
    pub fn check_xml_size(self, size: usize) -> Result<(), InterchangeLimitError> {
        check_limit("XML document", size, self.max_xml_bytes)
    }

    /// Reject a ZIP package whose transport size exceeds the configured bound.
    pub fn check_zip_size(self, size: usize) -> Result<(), InterchangeLimitError> {
        check_limit("ZIP package", size, self.max_zip_bytes)
    }
}

/// A stable, non-parser-specific limit violation for interchange diagnostics.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InterchangeLimitError {
    pub subject: &'static str,
    pub actual: usize,
    pub maximum: usize,
}

impl fmt::Display for InterchangeLimitError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "{} is {} bytes, exceeding the {} byte limit",
            self.subject, self.actual, self.maximum
        )
    }
}

impl std::error::Error for InterchangeLimitError {}

fn check_limit(
    subject: &'static str,
    actual: usize,
    maximum: usize,
) -> Result<(), InterchangeLimitError> {
    if actual > maximum {
        return Err(InterchangeLimitError {
            subject,
            actual,
            maximum,
        });
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn v0_1_identifiers_are_frozen() {
        assert_eq!(XCCDF_1_1_NAMESPACE, "http://checklists.nist.gov/xccdf/1.1");
        assert_eq!(XCCDF_1_2_NAMESPACE, "http://checklists.nist.gov/xccdf/1.2");
        // XCCDF_NAMESPACE always points to the canonical CF export namespace (1.2).
        assert_eq!(XCCDF_NAMESPACE, "http://checklists.nist.gov/xccdf/1.2");
        assert_eq!(CF_XCCDF_NAMESPACE, "urn:crystal-forge:xccdf:1");
        assert_eq!(
            CF_POLICY_CHECK_SYSTEM,
            "urn:crystal-forge:check-system:policy:1"
        );
        assert_eq!(CF_NIX_FIX_SYSTEM, "urn:crystal-forge:fix-system:nix:1");
        assert_eq!(CANONICALIZATION_VERSION, "cf-model-json-1");
        assert_eq!(DIGEST_ALGORITHM, "sha-256");
    }

    #[test]
    fn transport_limits_accept_the_boundary_and_reject_overflow() {
        let limits = InterchangeLimits::default();

        assert!(limits.check_xml_size(limits.max_xml_bytes).is_ok());
        assert_eq!(
            limits.check_xml_size(limits.max_xml_bytes + 1),
            Err(InterchangeLimitError {
                subject: "XML document",
                actual: limits.max_xml_bytes + 1,
                maximum: limits.max_xml_bytes,
            })
        );
        assert!(limits.check_zip_size(limits.max_zip_bytes).is_ok());
        assert!(limits.check_zip_size(limits.max_zip_bytes + 1).is_err());
    }
}
