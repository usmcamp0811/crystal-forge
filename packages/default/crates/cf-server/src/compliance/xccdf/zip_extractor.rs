//! Secure ZIP extraction for CF-XCCDF package preview.
//!
//! Extracts an XCCDF XML document from a bounded ZIP archive while enforcing
//! every security control required by the interchange specification:
//!
//! * Path-traversal entries are rejected.
//! * Symlinks are rejected.
//! * Nested archives (`.zip` inside a `.zip`) are rejected.
//! * The expansion ratio is checked against [`MAX_EXPANSION_RATIO`] to stop
//!   ZIP-bomb attacks.
//! * Total expanded bytes, individual file bytes, and file count are bounded.
//! * Exactly one XCCDF XML file must be present. Ambiguous archives with
//!   multiple XML files produce a blocking error.

use std::io::Read;

use zip::ZipArchive;

use super::super::interchange::InterchangeLimits;

// ── Constants ─────────────────────────────────────────────────────────────────

/// Maximum allowed expansion ratio (uncompressed / compressed) for the whole
/// archive. 100× is already generous for XML-heavy content; XCCDF benchmarks
/// typically compress 2–10×.
const MAX_EXPANSION_RATIO: u64 = 100;

// ── Public API ────────────────────────────────────────────────────────────────

/// Outcome of a successful extraction.
#[derive(Debug)]
pub struct ExtractedXccdf {
    /// Raw XML bytes for the chosen XCCDF document.
    pub xml_bytes: Vec<u8>,
    /// The name of the entry that was selected (used for diagnostics and the
    /// `source_filename` field).
    pub entry_name: String,
    /// Total number of entries that were inspected.
    pub total_entries: usize,
}

/// A structured error that can be surfaced as a typed interchange diagnostic.
#[derive(Debug)]
pub struct ZipExtractionError {
    /// Stable diagnostic code.
    pub code: &'static str,
    /// Human-readable explanation.
    pub message: String,
    /// Whether this error must block the import (always true here).
    pub blocking: bool,
}

impl ZipExtractionError {
    fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            blocking: true,
        }
    }
}

/// Extract the XCCDF XML document from `bytes`, enforcing all security limits
/// from `limits`.
///
/// Returns [`ExtractedXccdf`] on success, or a [`ZipExtractionError`] on any
/// security, structural, or selection failure.
pub fn extract_xccdf_from_zip(
    bytes: &[u8],
    limits: &InterchangeLimits,
) -> Result<ExtractedXccdf, ZipExtractionError> {
    // Transport-size check is the caller's responsibility (already done before
    // reaching here), but we enforce expanded limits inside the archive.

    let cursor = std::io::Cursor::new(bytes);
    let mut archive = ZipArchive::new(cursor).map_err(|e| {
        ZipExtractionError::new("ZIP_INVALID", format!("Cannot open ZIP archive: {e}"))
    })?;

    let total_entries = archive.len();
    if total_entries > limits.max_archive_files {
        return Err(ZipExtractionError::new(
            "ZIP_FILE_COUNT_EXCEEDED",
            format!(
                "Archive contains {total_entries} entries, exceeding the maximum of {}",
                limits.max_archive_files
            ),
        ));
    }

    // Cheap upfront ZIP-bomb check: compare total declared uncompressed size
    // (from the central directory) against total compressed size. This catches
    // bombs before any decompression occurs.
    let total_compressed: u64 = {
        let mut sum = 0u64;
        for i in 0..total_entries {
            if let Ok(e) = archive.by_index_raw(i) {
                sum = sum.saturating_add(e.compressed_size());
            }
        }
        sum
    };

    if let Some(total_uncompressed) = archive.decompressed_size() {
        if total_compressed > 0 {
            let ratio = (total_uncompressed as u64).saturating_div(total_compressed.max(1));
            if ratio > MAX_EXPANSION_RATIO {
                return Err(ZipExtractionError::new(
                    "ZIP_BOMB",
                    format!(
                        "Archive declares a {ratio}× expansion ratio, \
                         exceeding the maximum of {MAX_EXPANSION_RATIO}×"
                    ),
                ));
            }
        }
        if total_uncompressed > limits.max_expanded_archive_bytes as u128 {
            return Err(ZipExtractionError::new(
                "ZIP_EXPANDED_SIZE_EXCEEDED",
                format!(
                    "Archive declares {} uncompressed bytes, exceeding the limit of {}",
                    total_uncompressed, limits.max_expanded_archive_bytes
                ),
            ));
        }
    }

    // Collect candidate XML entries, enforcing per-entry and per-archive
    // security checks.
    let mut xml_candidates: Vec<(usize, String)> = Vec::new();
    let mut cumulative_expanded: u64 = 0;

    for i in 0..total_entries {
        let entry = archive.by_index(i).map_err(|e| {
            ZipExtractionError::new(
                "ZIP_ENTRY_ERROR",
                format!("Cannot read archive entry {i}: {e}"),
            )
        })?;

        // ── Security checks on each entry ─────────────────────────────────

        // Reject symlinks unconditionally.
        if entry.is_symlink() {
            return Err(ZipExtractionError::new(
                "ZIP_SYMLINK",
                format!(
                    "Archive entry '{}' is a symbolic link; symlinks are not permitted",
                    entry.name()
                ),
            ));
        }

        // Reject path traversal.
        if entry.enclosed_name().is_none() && !entry.is_dir() {
            return Err(ZipExtractionError::new(
                "ZIP_PATH_TRAVERSAL",
                format!(
                    "Archive entry '{}' contains a path traversal sequence",
                    entry.name()
                ),
            ));
        }

        // Skip directory entries.
        if entry.is_dir() {
            continue;
        }

        let name = entry.name().to_string();
        let name_lower = name.to_lowercase();

        // Reject nested archives.
        if name_lower.ends_with(".zip") {
            return Err(ZipExtractionError::new(
                "ZIP_NESTED_ARCHIVE",
                format!(
                    "Archive entry '{name}' is a nested ZIP archive; nested archives are not permitted"
                ),
            ));
        }

        // Per-entry expansion ratio (entry.size() is the declared uncompressed size).
        let entry_uncompressed = entry.size();
        let entry_compressed = entry.compressed_size().max(1);
        let entry_ratio = entry_uncompressed / entry_compressed;
        if entry_ratio > MAX_EXPANSION_RATIO {
            return Err(ZipExtractionError::new(
                "ZIP_BOMB",
                format!(
                    "Archive entry '{name}' has an expansion ratio of {entry_ratio}×, \
                     exceeding the maximum of {MAX_EXPANSION_RATIO}×"
                ),
            ));
        }

        cumulative_expanded = cumulative_expanded.saturating_add(entry_uncompressed);

        // Cumulative expanded-bytes limit (defence-in-depth after the upfront check).
        if cumulative_expanded > limits.max_expanded_archive_bytes as u64 {
            return Err(ZipExtractionError::new(
                "ZIP_EXPANDED_SIZE_EXCEEDED",
                format!(
                    "Archive expands to at least {cumulative_expanded} bytes, \
                     exceeding the limit of {}",
                    limits.max_expanded_archive_bytes
                ),
            ));
        }

        // Collect XML candidates for later selection.
        if name_lower.ends_with(".xml") {
            xml_candidates.push((i, name));
        }
    }

    // ── XML candidate selection ───────────────────────────────────────────────

    if xml_candidates.is_empty() {
        return Err(ZipExtractionError::new(
            "ZIP_NO_XML",
            "Archive contains no .xml files; expected exactly one XCCDF document",
        ));
    }

    // Prefer the shortest path (closest to the root) as the primary XCCDF
    // document. If there is still ambiguity after filtering, reject.
    let chosen_index = if xml_candidates.len() == 1 {
        xml_candidates[0].0
    } else {
        // Try to find the one root-level XML (no directory separator).
        let root_candidates: Vec<_> = xml_candidates
            .iter()
            .filter(|(_, name)| !name.contains('/') && !name.contains('\\'))
            .collect();

        match root_candidates.len() {
            1 => root_candidates[0].0,
            0 => {
                // Fall back to the shallowest path.
                let min_depth = xml_candidates
                    .iter()
                    .map(|(_, n)| n.chars().filter(|&c| c == '/').count())
                    .min()
                    .unwrap_or(0);
                let shallowest: Vec<_> = xml_candidates
                    .iter()
                    .filter(|(_, n)| n.chars().filter(|&c| c == '/').count() == min_depth)
                    .collect();
                if shallowest.len() == 1 {
                    shallowest[0].0
                } else {
                    return Err(ZipExtractionError::new(
                        "ZIP_AMBIGUOUS_XML",
                        format!(
                            "Archive contains {} .xml files at the same depth; \
                             include exactly one XCCDF document",
                            xml_candidates.len()
                        ),
                    ));
                }
            }
            _ => {
                return Err(ZipExtractionError::new(
                    "ZIP_AMBIGUOUS_XML",
                    format!(
                        "Archive contains {} root-level .xml files; \
                         include exactly one XCCDF document",
                        root_candidates.len()
                    ),
                ));
            }
        }
    };

    let chosen_name = xml_candidates
        .iter()
        .find(|(i, _)| *i == chosen_index)
        .map(|(_, n)| n.clone())
        .unwrap_or_default();

    // ── Read the selected entry ───────────────────────────────────────────────

    let mut entry = archive.by_index(chosen_index).map_err(|e| {
        ZipExtractionError::new(
            "ZIP_ENTRY_ERROR",
            format!("Cannot open selected entry '{chosen_name}': {e}"),
        )
    })?;

    let mut xml_bytes = Vec::new();
    let mut buf = [0u8; 65536];
    loop {
        let n = entry.read(&mut buf).map_err(|e| {
            ZipExtractionError::new(
                "ZIP_READ_ERROR",
                format!("Error reading '{chosen_name}': {e}"),
            )
        })?;
        if n == 0 {
            break;
        }
        xml_bytes.extend_from_slice(&buf[..n]);
        if xml_bytes.len() > limits.max_xml_bytes {
            return Err(ZipExtractionError::new(
                "ZIP_XML_TOO_LARGE",
                format!(
                    "Extracted XML from '{chosen_name}' exceeds the {} byte limit",
                    limits.max_xml_bytes
                ),
            ));
        }
    }

    Ok(ExtractedXccdf {
        xml_bytes,
        entry_name: chosen_name,
        total_entries,
    })
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use zip::CompressionMethod;
    use zip::write::{FileOptions, SimpleFileOptions};

    fn limits() -> InterchangeLimits {
        InterchangeLimits::default()
    }

    fn make_zip(files: &[(&str, &[u8])]) -> Vec<u8> {
        let mut buf = Vec::new();
        {
            let mut w = zip::ZipWriter::new(std::io::Cursor::new(&mut buf));
            let opts: SimpleFileOptions =
                FileOptions::default().compression_method(CompressionMethod::Stored);
            for (name, content) in files {
                w.start_file(*name, opts).expect("start_file");
                w.write_all(content).expect("write_all");
            }
            w.finish().expect("zip finish");
        }
        buf
    }

    const MINIMAL_XML: &[u8] = b"<?xml version=\"1.0\"?><Benchmark xmlns=\"http://checklists.nist.gov/xccdf/1.2\" id=\"b1\"><status>draft</status><title>T</title><version>0.1</version></Benchmark>";

    #[test]
    fn extracts_single_xml_entry() {
        let zip = make_zip(&[("benchmark.xml", MINIMAL_XML)]);
        let result = extract_xccdf_from_zip(&zip, &limits()).unwrap();
        assert_eq!(result.xml_bytes, MINIMAL_XML);
        assert_eq!(result.entry_name, "benchmark.xml");
    }

    #[test]
    fn prefers_root_xml_over_subdirectory_xml() {
        let zip = make_zip(&[
            ("subfolder/other.xml", b"<other/>"),
            ("benchmark.xml", MINIMAL_XML),
        ]);
        let result = extract_xccdf_from_zip(&zip, &limits()).unwrap();
        assert_eq!(result.entry_name, "benchmark.xml");
    }

    #[test]
    fn rejects_empty_zip() {
        let zip = make_zip(&[]);
        let err = extract_xccdf_from_zip(&zip, &limits()).unwrap_err();
        assert_eq!(err.code, "ZIP_NO_XML");
    }

    #[test]
    fn rejects_zip_with_no_xml() {
        let zip = make_zip(&[("readme.txt", b"hello"), ("data.json", b"{}")]);
        let err = extract_xccdf_from_zip(&zip, &limits()).unwrap_err();
        assert_eq!(err.code, "ZIP_NO_XML");
    }

    #[test]
    fn rejects_ambiguous_multiple_root_xml() {
        let zip = make_zip(&[("a.xml", MINIMAL_XML), ("b.xml", MINIMAL_XML)]);
        let err = extract_xccdf_from_zip(&zip, &limits()).unwrap_err();
        assert_eq!(err.code, "ZIP_AMBIGUOUS_XML");
    }

    #[test]
    fn rejects_nested_zip() {
        let inner = make_zip(&[("x.xml", MINIMAL_XML)]);
        let outer = make_zip(&[("inner.zip", &inner), ("outer.xml", MINIMAL_XML)]);
        let err = extract_xccdf_from_zip(&outer, &limits()).unwrap_err();
        assert_eq!(err.code, "ZIP_NESTED_ARCHIVE");
    }

    #[test]
    fn rejects_file_count_above_limit() {
        // Build a zip with max_archive_files+1 xml entries (use subdirs to avoid ambiguity).
        let small_limit = InterchangeLimits {
            max_archive_files: 3,
            ..InterchangeLimits::default()
        };
        let zip = make_zip(&[
            ("a/1.xml", b"<x/>"),
            ("b/2.xml", b"<x/>"),
            ("c/3.xml", b"<x/>"),
            ("root.xml", MINIMAL_XML),
        ]);
        let err = extract_xccdf_from_zip(&zip, &small_limit).unwrap_err();
        assert_eq!(err.code, "ZIP_FILE_COUNT_EXCEEDED");
    }

    #[test]
    fn rejects_xml_too_large_after_extraction() {
        let small_limit = InterchangeLimits {
            max_xml_bytes: 10,
            ..InterchangeLimits::default()
        };
        let zip = make_zip(&[("benchmark.xml", MINIMAL_XML)]);
        // MINIMAL_XML is well over 10 bytes.
        let err = extract_xccdf_from_zip(&zip, &small_limit).unwrap_err();
        assert_eq!(err.code, "ZIP_XML_TOO_LARGE");
    }

    #[test]
    fn rejects_invalid_zip_bytes() {
        let err = extract_xccdf_from_zip(b"not a zip at all", &limits()).unwrap_err();
        assert_eq!(err.code, "ZIP_INVALID");
    }

    #[test]
    fn accepts_single_xml_in_subdirectory() {
        let zip = make_zip(&[("folder/benchmark.xml", MINIMAL_XML)]);
        let result = extract_xccdf_from_zip(&zip, &limits()).unwrap();
        assert_eq!(result.entry_name, "folder/benchmark.xml");
    }

    #[test]
    fn accepts_non_xml_files_alongside_single_xml() {
        let zip = make_zip(&[
            ("readme.txt", b"description"),
            ("benchmark.xml", MINIMAL_XML),
            ("sig.asc", b"signature"),
        ]);
        let result = extract_xccdf_from_zip(&zip, &limits()).unwrap();
        assert_eq!(result.entry_name, "benchmark.xml");
    }
}
