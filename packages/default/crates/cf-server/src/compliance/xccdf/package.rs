//! Shared XCCDF package processing for preview and import.
//!
//! Both the preview and import handlers accept the same XML or ZIP content.
//! This module provides a single processing pipeline so that preview and import
//! always see identical parsing results for the same bytes.
//!
//! Authentication, HTTP response construction, and multipart field routing
//! remain in the handlers.

use anyhow::Result;
use sha2::{Digest as _, Sha256};

use super::super::interchange::{InterchangeLimits, MAX_XCCDF_XML_BYTES};
use super::models::ParsedXccdf;
use super::parser::parse_xccdf;
use super::zip_extractor::{PackageKind, detect_package_kind, extract_xccdf_from_zip};

/// Detailed package-source provenance for UI or audit display.
#[derive(Debug, Clone)]
pub struct PackageProvenance {
    /// Original bytes: either the raw XML or the full ZIP archive.
    pub package_kind: PackageKind,
    /// SHA-256 hex of the original uploaded bytes.
    pub sha256: String,
    /// Original byte count.
    pub size_bytes: usize,
    /// Original filename from the multipart `Content-Disposition`.
    pub filename: Option<String>,
    /// The ZIP entry selected as the XCCDF source, if the upload was a ZIP.
    pub selected_entry: Option<String>,
    /// SHA-256 hex of the selected XML bytes (may differ from [`sha256`] when
    /// the upload was a ZIP and a single entry was extracted).
    pub selected_xml_sha256: Option<String>,
    /// Number of files in the archive, when the upload was a ZIP.
    pub archive_file_count: Option<usize>,
}

/// The complete result of processing one XCCDF upload.
///
/// Both the preview and import handlers construct this from the same multipart
/// field bytes before diverging into their own response paths.
pub struct ProcessedXccdfPackage {
    /// Original uploaded bytes (the ZIP or direct XML).
    pub original_bytes: Vec<u8>,
    /// Provenance metadata for the original package.
    pub provenance: PackageProvenance,
    /// Parsed XCCDF document.
    pub parsed: ParsedXccdf,
}

/// Errors that can occur during package detection, extraction, or parsing.
///
/// The variant carries enough context for the handler to construct the
/// appropriate HTTP status code without inspecting message strings.
#[derive(Debug)]
pub enum ProcessingError {
    /// Content does not match any known package type (XML or ZIP).
    UnknownContentType,
    /// The file extension contradicts the detected content type.
    ContentExtensionMismatch { reason: &'static str },
    /// The uploaded package exceeds the applicable size limit.
    TooLarge {
        subject: &'static str,
        actual: usize,
        maximum: usize,
    },
    /// ZIP extraction failed.
    ZipExtraction {
        code: &'static str,
        message: String,
        http_status: u16,
        candidates: Vec<String>,
    },
    /// XCCDF parsing returned one or more blocking diagnostics.
    BlockingDiagnostics { parsed: Box<ParsedXccdf> },
    /// An internal IO or structural failure (not a user error).
    Internal(anyhow::Error),
}

impl From<anyhow::Error> for ProcessingError {
    fn from(e: anyhow::Error) -> Self {
        Self::Internal(e)
    }
}

/// Process raw upload bytes through the full detection → extraction → parse pipeline.
///
/// Returns the complete result, including the original bytes (preserved for the
/// source artifact) and all provenance metadata.
///
/// This function is independent of PostgreSQL and HTTP; it can be called from
/// both the preview handler (which reads bytes from a multipart upload) and the
/// import handler (which may need to reconstruct bytes from stored state).
pub fn process_xccdf_bytes(
    bytes: Vec<u8>,
    filename: Option<String>,
    limits: &InterchangeLimits,
) -> Result<ProcessedXccdfPackage, ProcessingError> {
    if bytes.is_empty() {
        return Err(ProcessingError::UnknownContentType);
    }

    let original_sha256 = hex::encode(Sha256::digest(&bytes));
    let original_size = bytes.len();
    let package_kind = detect_package_kind(&bytes).ok_or(ProcessingError::UnknownContentType)?;

    // Extension coherence: reject explicit contradictions, accept no-extension.
    let file_ext = filename
        .as_deref()
        .and_then(|f| f.rsplit('/').next())
        .and_then(|seg| {
            let dot_pos = seg.rfind('.')?;
            if dot_pos == 0 {
                None
            } else {
                Some(&seg[dot_pos + 1..])
            }
        })
        .map(|e| e.to_lowercase());
    let has_xml_ext = file_ext.as_deref() == Some("xml");
    let has_zip_ext = file_ext.as_deref() == Some("zip");
    let has_wrong_ext = file_ext.is_some() && !has_xml_ext && !has_zip_ext;

    let mismatch: Option<&'static str> = if has_wrong_ext {
        Some("file extension is not .xml or .zip")
    } else {
        match package_kind {
            PackageKind::Zip if has_xml_ext => Some("ZIP bytes uploaded with an .xml extension"),
            PackageKind::Xml if has_zip_ext => Some("XML bytes uploaded with a .zip extension"),
            _ => None,
        }
    };
    if let Some(reason) = mismatch {
        return Err(ProcessingError::ContentExtensionMismatch { reason });
    }

    // Per-type size check.
    match package_kind {
        PackageKind::Xml => {
            if original_size > MAX_XCCDF_XML_BYTES {
                return Err(ProcessingError::TooLarge {
                    subject: "XML",
                    actual: original_size,
                    maximum: MAX_XCCDF_XML_BYTES,
                });
            }
        }
        PackageKind::Zip => {
            if let Err(e) = limits.check_zip_size(original_size) {
                return Err(ProcessingError::TooLarge {
                    subject: e.subject,
                    actual: e.actual,
                    maximum: e.maximum,
                });
            }
        }
    }

    // ZIP extraction or direct XML passthrough.
    let (xml_bytes, xml_filename, selected_entry, selected_xml_sha256, archive_file_count) =
        match package_kind {
            PackageKind::Zip => match extract_xccdf_from_zip(&bytes, limits) {
                Ok(extracted) => {
                    let xml_sha256 = extracted.xml_sha256.clone();
                    let entry = extracted.entry_name.clone();
                    let count = extracted.archive_file_count;
                    let xml_name = Some(extracted.entry_name.clone());
                    (
                        extracted.xml_bytes,
                        xml_name,
                        Some(entry),
                        Some(xml_sha256),
                        Some(count),
                    )
                }
                Err(e) => {
                    return Err(ProcessingError::ZipExtraction {
                        code: e.code,
                        message: e.message,
                        http_status: e.http_status,
                        candidates: e.candidates,
                    });
                }
            },
            PackageKind::Xml => (bytes.clone(), filename.clone(), None, None, None),
        };

    // Parse the XML.
    let parsed = match parse_xccdf(&xml_bytes, xml_filename.as_deref(), limits) {
        Ok(p) => p,
        Err(e) => return Err(ProcessingError::Internal(e)),
    };

    // Blocking diagnostics prevent both preview success and committed import.
    if parsed.errors.iter().any(|e| e.blocking) {
        return Err(ProcessingError::BlockingDiagnostics {
            parsed: Box::new(parsed),
        });
    }

    let provenance = PackageProvenance {
        package_kind,
        sha256: original_sha256,
        size_bytes: original_size,
        filename,
        selected_entry,
        selected_xml_sha256,
        archive_file_count,
    };

    Ok(ProcessedXccdfPackage {
        original_bytes: bytes,
        provenance,
        parsed,
    })
}

/// Return the `package_context` JSONB value to store in `compliance_source_artifacts`.
pub fn build_package_context(provenance: &PackageProvenance) -> serde_json::Value {
    match provenance.package_kind {
        PackageKind::Xml => serde_json::json!({
            "package_kind": "direct_xml",
            "original_filename": provenance.filename,
            "original_size": provenance.size_bytes,
            "original_sha256": provenance.sha256,
        }),
        PackageKind::Zip => serde_json::json!({
            "package_kind": "zip_package",
            "original_filename": provenance.filename,
            "original_size": provenance.size_bytes,
            "original_sha256": provenance.sha256,
            "selected_entry": provenance.selected_entry,
            "selected_xml_sha256": provenance.selected_xml_sha256,
            "archive_file_count": provenance.archive_file_count,
        }),
    }
}

/// Return the `package_context` JSON as a `serde_json::Value` in the shape used by
/// the legacy preview response `"source"` field.
pub fn build_preview_source_json(
    provenance: &PackageProvenance,
    xml_filename: Option<&str>,
) -> serde_json::Value {
    match provenance.package_kind {
        PackageKind::Xml => serde_json::json!({
            "package_kind": "direct_xml",
            "original_filename": provenance.filename,
            "original_size": provenance.size_bytes,
            "original_sha256": provenance.sha256,
        }),
        PackageKind::Zip => serde_json::json!({
            "package_kind": "zip_package",
            "original_filename": provenance.filename,
            "original_size": provenance.size_bytes,
            "original_sha256": provenance.sha256,
            "selected_entry": provenance.selected_entry,
            "selected_xml_sha256": provenance.selected_xml_sha256,
            "archive_file_count": provenance.archive_file_count,
        }),
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn minimal_xccdf_bytes() -> Vec<u8> {
        br#"<?xml version="1.0" encoding="UTF-8"?>
<Benchmark xmlns="http://checklists.nist.gov/xccdf/1.2"
    id="xccdf_org.test_benchmark">
  <status>draft</status>
  <title>Test</title>
  <version>1.0</version>
  <Rule id="xccdf_org.test_rule_001">
    <title>Test rule</title>
    <check system="urn:test:check">
      <check-content>Verify the setting.</check-content>
    </check>
  </Rule>
</Benchmark>"#
            .to_vec()
    }

    #[test]
    fn processes_valid_xml_bytes() {
        let bytes = minimal_xccdf_bytes();
        let sha256 = hex::encode(Sha256::digest(&bytes));
        let result = process_xccdf_bytes(
            bytes,
            Some("test.xml".into()),
            &InterchangeLimits::default(),
        )
        .expect("should succeed");

        assert_eq!(result.provenance.sha256, sha256);
        assert_eq!(result.provenance.package_kind, PackageKind::Xml);
        assert_eq!(result.provenance.selected_entry, None);
        assert_eq!(result.parsed.rules.len(), 1);
        assert_eq!(result.parsed.rules[0].id, "xccdf_org.test_rule_001");
    }

    #[test]
    fn same_bytes_produce_same_digest_regardless_of_caller() {
        let bytes = minimal_xccdf_bytes();
        let expected_sha256 = hex::encode(Sha256::digest(&bytes));

        // Call twice (simulating preview then import).
        let r1 = process_xccdf_bytes(
            bytes.clone(),
            Some("test.xml".into()),
            &InterchangeLimits::default(),
        )
        .expect("first call");
        let r2 = process_xccdf_bytes(
            bytes,
            Some("test.xml".into()),
            &InterchangeLimits::default(),
        )
        .expect("second call");

        assert_eq!(r1.provenance.sha256, expected_sha256);
        assert_eq!(r2.provenance.sha256, expected_sha256);
        assert_eq!(r1.parsed.rules.len(), r2.parsed.rules.len());
        assert_eq!(r1.parsed.rules[0].id, r2.parsed.rules[0].id);
    }

    #[test]
    fn rejects_empty_bytes() {
        let result = process_xccdf_bytes(vec![], None, &InterchangeLimits::default());
        assert!(matches!(result, Err(ProcessingError::UnknownContentType)));
    }

    #[test]
    fn rejects_unknown_binary_bytes() {
        let bytes = vec![0xde, 0xad, 0xbe, 0xef, 0x00, 0x01, 0x02, 0x03];
        let result = process_xccdf_bytes(bytes, None, &InterchangeLimits::default());
        assert!(matches!(result, Err(ProcessingError::UnknownContentType)));
    }

    #[test]
    fn rejects_xml_with_zip_extension() {
        let bytes = minimal_xccdf_bytes();
        let result = process_xccdf_bytes(
            bytes,
            Some("docs.zip".into()),
            &InterchangeLimits::default(),
        );
        assert!(matches!(
            result,
            Err(ProcessingError::ContentExtensionMismatch { .. })
        ));
    }

    #[test]
    fn xml_without_extension_is_accepted() {
        let bytes = minimal_xccdf_bytes();
        let result = process_xccdf_bytes(
            bytes,
            Some("nosuffix".into()),
            &InterchangeLimits::default(),
        );
        assert!(result.is_ok(), "no extension should be accepted");
    }
}
