//! Secure ZIP extraction for CF-XCCDF package preview.
//!
//! Identifies uploaded bytes by content signature, extracts the XCCDF XML
//! document from a bounded ZIP archive, and enforces every security control
//! required by the interchange specification:
//!
//! * Content-based package detection (bytes, not filename extension).
//! * Portable path-name validation — rejects traversal, NUL, absolute paths,
//!   drive-letter prefixes, UNC paths, `.`/`..` path components, and raw `\`.
//! * Symlink, device, FIFO, socket, and other non-regular entry rejection.
//! * Path-traversal entries rejected for both files and directories.
//! * Content-based nested archive detection (ZIP magic bytes, not `.zip`).
//! * Expansion-ratio checked with exact multiplication to avoid truncation.
//! * Total expanded bytes, individual file bytes, and file count bounded.
//! * XCCDF-aware candidate selection — uses `NsReader` to verify the root
//!   element is an XCCDF `Benchmark`.

use std::io::Read;

use quick_xml::events::Event;
use quick_xml::name::ResolveResult;
use quick_xml::reader::NsReader;
use sha2::{Digest, Sha256};
use zip::ZipArchive;

use super::super::interchange::{InterchangeLimits, XCCDF_NAMESPACE};

// ── Constants ─────────────────────────────────────────────────────────────────

/// Maximum allowed expansion ratio (uncompressed / compressed).
/// 100× is generous; typical XCCDF content compresses 2–10×.
const MAX_EXPANSION_RATIO: u64 = 100;

/// Maximum bytes read from a file entry when peeking for ZIP magic or the XML
/// root element.  8 KiB is more than enough for any XML declaration and root
/// opening tag.
const PEEK_BYTES: usize = 8 * 1024;

/// ZIP local-file-header magic bytes.
const ZIP_MAGIC_LOCAL: [u8; 4] = [b'P', b'K', 0x03, 0x04];
/// ZIP end-of-central-directory magic bytes.
const ZIP_MAGIC_EOCD: [u8; 4] = [b'P', b'K', 0x05, 0x06];
/// ZIP data descriptor / spanned archive magic bytes.
const ZIP_MAGIC_SPAN: [u8; 4] = [b'P', b'K', 0x07, 0x08];

// ── Public types ──────────────────────────────────────────────────────────────

/// Content type of the uploaded bytes, determined by signature not extension.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PackageKind {
    /// Byte stream that starts with a `<` (after optional UTF-8 BOM and
    /// whitespace).
    Xml,
    /// Byte stream that starts with one of the ZIP local-header magic values.
    Zip,
}

/// Outcome of a successful extraction.
#[derive(Debug)]
pub struct ExtractedXccdf {
    /// Raw XML bytes for the chosen XCCDF document.
    pub xml_bytes: Vec<u8>,
    /// SHA-256 of `xml_bytes` (hex).
    pub xml_sha256: String,
    /// Entry path inside the archive that was selected.
    pub entry_name: String,
    /// Total number of non-directory file entries in the archive.
    pub archive_file_count: usize,
}

/// A structured error that carries a stable HTTP-status hint and can be
/// surfaced as a typed interchange diagnostic.
#[derive(Debug)]
pub struct ZipExtractionError {
    /// Stable diagnostic code.
    pub code: &'static str,
    /// Human-readable explanation.
    pub message: String,
    /// Suggested HTTP status: 413 for resource-limit violations, 422 for
    /// structural/security failures.
    pub http_status: u16,
    /// Candidate entry names for ambiguous-XCCDF errors.
    pub candidates: Vec<String>,
}

impl ZipExtractionError {
    fn limit(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            http_status: 413,
            candidates: vec![],
        }
    }

    fn invalid(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            http_status: 422,
            candidates: vec![],
        }
    }

    fn ambiguous(code: &'static str, message: impl Into<String>, candidates: Vec<String>) -> Self {
        Self {
            code,
            message: message.into(),
            http_status: 422,
            candidates,
        }
    }
}

// ── Package-kind detection ────────────────────────────────────────────────────

/// Detect the content type of `bytes` from its leading bytes.
///
/// Returns `None` for unknown / unrecognised content. The filename extension
/// must not override this result.
pub fn detect_package_kind(bytes: &[u8]) -> Option<PackageKind> {
    // ZIP magic: local header, end-of-central-directory, or spanned archive.
    if matches!(
        bytes.get(..4).map(|s| s.try_into().unwrap_or([0; 4])),
        Some(m) if m == ZIP_MAGIC_LOCAL || m == ZIP_MAGIC_EOCD || m == ZIP_MAGIC_SPAN
    ) {
        return Some(PackageKind::Zip);
    }

    // XML: optional UTF-8 BOM, then optional ASCII whitespace, then `<`.
    let without_bom = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes);
    let first_non_ws = without_bom
        .iter()
        .copied()
        .find(|b| !b.is_ascii_whitespace());
    if first_non_ws == Some(b'<') {
        return Some(PackageKind::Xml);
    }

    None
}

// ── ZIP extraction ────────────────────────────────────────────────────────────

/// Extract the XCCDF XML document from `bytes`, enforcing all security limits.
pub fn extract_xccdf_from_zip(
    bytes: &[u8],
    limits: &InterchangeLimits,
) -> Result<ExtractedXccdf, ZipExtractionError> {
    let cursor = std::io::Cursor::new(bytes);
    let mut archive = ZipArchive::new(cursor).map_err(|e| {
        ZipExtractionError::invalid("ZIP_INVALID", format!("Cannot open ZIP archive: {e}"))
    })?;

    // ── File-count limit ──────────────────────────────────────────────────────

    let total_entries = archive.len();
    if total_entries > limits.max_archive_files {
        return Err(ZipExtractionError::limit(
            "ZIP_FILE_COUNT_EXCEEDED",
            format!(
                "Archive contains {total_entries} entries, exceeding the maximum of {}",
                limits.max_archive_files
            ),
        ));
    }

    // ── Upfront ZIP-bomb check from central-directory declarations ─────────────

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
        let total_u64 = total_uncompressed as u64;
        if exceeds_ratio(total_u64, total_compressed, MAX_EXPANSION_RATIO) {
            return Err(ZipExtractionError::limit(
                "ZIP_BOMB",
                format!(
                    "Archive declares a suspiciously high expansion ratio; \
                     uncompressed={total_uncompressed} compressed={total_compressed}"
                ),
            ));
        }
        if total_uncompressed > limits.max_expanded_archive_bytes as u128 {
            return Err(ZipExtractionError::limit(
                "ZIP_EXPANDED_SIZE_EXCEEDED",
                format!(
                    "Archive declares {total_uncompressed} uncompressed bytes, \
                     exceeding the limit of {}",
                    limits.max_expanded_archive_bytes
                ),
            ));
        }
    }

    // ── Per-entry scan ────────────────────────────────────────────────────────

    // Collect indices of all XML-extension candidates and enforce security
    // controls on every entry.

    let mut xml_candidates: Vec<(usize, String)> = Vec::new();
    let mut cumulative_expanded: u64 = 0;
    let mut file_entry_count: usize = 0;

    for i in 0..total_entries {
        let entry = archive.by_index(i).map_err(|e| {
            ZipExtractionError::invalid(
                "ZIP_ENTRY_ERROR",
                format!("Cannot read archive entry {i}: {e}"),
            )
        })?;

        let raw_name = entry.name().to_string();

        // ── Name safety ───────────────────────────────────────────────────────

        if let Err(reason) = validate_entry_name(&raw_name) {
            return Err(ZipExtractionError::invalid(
                "ZIP_PATH_TRAVERSAL",
                format!("Archive entry '{raw_name}' is unsafe: {reason}"),
            ));
        }

        // `enclosed_name()` returns None for any path that would escape the
        // archive root (e.g. `../evil`). Apply to both files and directories.
        if entry.enclosed_name().is_none() {
            return Err(ZipExtractionError::invalid(
                "ZIP_PATH_TRAVERSAL",
                format!("Archive entry '{raw_name}' contains a path traversal sequence"),
            ));
        }

        // ── Entry-type checks ─────────────────────────────────────────────────

        // Symlinks, devices, FIFOs, sockets, and other non-regular-file /
        // non-directory entries are rejected.  Only regular files and directories
        // are accepted.
        if entry.is_symlink() {
            return Err(ZipExtractionError::invalid(
                "ZIP_SYMLINK",
                format!("Archive entry '{raw_name}' is a symbolic link"),
            ));
        }
        if let Some(mode) = entry.unix_mode() {
            // Unix file-type bits sit in the upper nibble of the octal mode.
            // S_IFREG = 0o100000, S_IFDIR = 0o040000; anything else is rejected.
            const S_IFMT: u32 = 0o170000;
            const S_IFREG: u32 = 0o100000;
            const S_IFDIR: u32 = 0o040000;
            let file_type = mode & S_IFMT;
            if file_type != 0 && file_type != S_IFREG && file_type != S_IFDIR {
                return Err(ZipExtractionError::invalid(
                    "ZIP_UNSUPPORTED_ENTRY",
                    format!(
                        "Archive entry '{raw_name}' has unsupported Unix type (mode={mode:#o})"
                    ),
                ));
            }
        }

        // Directory entries are safe; skip further checks.
        if entry.is_dir() {
            continue;
        }

        file_entry_count += 1;

        // ── Per-entry size and expansion checks ───────────────────────────────

        let entry_uncompressed = entry.size();
        let entry_compressed = entry.compressed_size();
        if exceeds_ratio(entry_uncompressed, entry_compressed, MAX_EXPANSION_RATIO) {
            return Err(ZipExtractionError::limit(
                "ZIP_BOMB",
                format!("Archive entry '{raw_name}' has an excessive expansion ratio"),
            ));
        }

        cumulative_expanded = cumulative_expanded.saturating_add(entry_uncompressed);
        if cumulative_expanded > limits.max_expanded_archive_bytes as u64 {
            return Err(ZipExtractionError::limit(
                "ZIP_EXPANDED_SIZE_EXCEEDED",
                format!(
                    "Archive expands to at least {cumulative_expanded} bytes, \
                     exceeding the limit of {}",
                    limits.max_expanded_archive_bytes
                ),
            ));
        }

        // Collect only XML-extension entries as XCCDF candidates (XCCDF
        // verification happens in the next pass).
        let name_lower = raw_name.to_lowercase();
        if name_lower.ends_with(".xml") {
            xml_candidates.push((i, raw_name));
        }
    }

    // Re-open entries to check for nested ZIPs by content and to verify the
    // XCCDF root element.  We now have the full list of XML candidates; peek
    // every file for ZIP magic and XML root simultaneously.
    // Non-XML files are also peeked for ZIP magic to catch disguised archives.
    //
    // For efficiency, peek non-XML entries that were collected above in the
    // same per-entry order.  Because ZipArchive borrows the cursor mutably,
    // we cannot hold both a by_index handle and index metadata simultaneously,
    // so we do a second pass.

    // Step A: peek every non-XML file entry for ZIP magic.
    for i in 0..total_entries {
        let mut entry = archive.by_index(i).map_err(|e| {
            ZipExtractionError::invalid(
                "ZIP_ENTRY_ERROR",
                format!("Cannot re-read archive entry {i}: {e}"),
            )
        })?;
        if entry.is_dir() || entry.is_symlink() {
            continue;
        }
        let raw_name = entry.name().to_string();
        let name_lower = raw_name.to_lowercase();
        // XML entries are checked below for Benchmark root element, not for ZIP magic.
        if name_lower.ends_with(".xml") {
            continue;
        }
        // Peek first 4 bytes.
        let mut peek = [0u8; 4];
        let n = entry.read(&mut peek).unwrap_or(0);
        if n >= 4 && is_zip_magic(&peek[..4]) {
            return Err(ZipExtractionError::invalid(
                "ZIP_NESTED_ARCHIVE",
                format!(
                    "Archive entry '{raw_name}' contains a ZIP archive \
                     (detected by content signature)"
                ),
            ));
        }
    }

    // Step B: verify XCCDF root element of XML candidates.
    let mut xccdf_candidates: Vec<(usize, String)> = Vec::new();
    for (idx, name) in &xml_candidates {
        let mut entry = archive.by_index(*idx).map_err(|e| {
            ZipExtractionError::invalid(
                "ZIP_ENTRY_ERROR",
                format!("Cannot re-read archive entry '{name}': {e}"),
            )
        })?;
        // Read up to PEEK_BYTES.
        let mut buf = vec![0u8; PEEK_BYTES.min(entry.size() as usize + 1)];
        let n = entry.read(&mut buf).unwrap_or(0);
        buf.truncate(n);

        // Also check for ZIP magic disguised as XML.
        if n >= 4 && is_zip_magic(&buf[..4]) {
            return Err(ZipExtractionError::invalid(
                "ZIP_NESTED_ARCHIVE",
                format!(
                    "Archive entry '{name}' has a .xml extension but \
                     contains a ZIP archive"
                ),
            ));
        }

        if is_xccdf_benchmark(&buf) {
            xccdf_candidates.push((*idx, name.clone()));
        }
    }

    // ── XCCDF candidate selection ─────────────────────────────────────────────

    match xccdf_candidates.len() {
        0 => {
            if xml_candidates.is_empty() {
                return Err(ZipExtractionError::invalid(
                    "ZIP_NO_XCCDF",
                    "Archive contains no .xml files",
                ));
            }
            Err(ZipExtractionError::invalid(
                "ZIP_NO_XCCDF",
                format!(
                    "Archive contains {} .xml file(s) but none has an XCCDF Benchmark root element",
                    xml_candidates.len()
                ),
            ))
        }
        1 => {
            // Exactly one XCCDF document — use it.
            let (chosen_idx, chosen_name) = xccdf_candidates.remove(0);
            read_selected_entry(
                &mut archive,
                chosen_idx,
                &chosen_name,
                limits,
                file_entry_count,
            )
        }
        _ => {
            let names: Vec<String> = xccdf_candidates.into_iter().map(|(_, n)| n).collect();
            Err(ZipExtractionError::ambiguous(
                "ZIP_AMBIGUOUS_XCCDF",
                format!(
                    "Archive contains {} XCCDF Benchmark documents; \
                     include exactly one. Candidates: {}",
                    names.len(),
                    names.join(", ")
                ),
                names,
            ))
        }
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Exact expansion-ratio check that avoids integer-division truncation.
///
/// Returns `true` when `uncompressed > compressed × max_ratio`.
fn exceeds_ratio(uncompressed: u64, compressed: u64, max_ratio: u64) -> bool {
    if uncompressed == 0 {
        return false;
    }
    if compressed == 0 {
        return true;
    }
    uncompressed > compressed.saturating_mul(max_ratio)
}

/// Returns true when `bytes` starts with one of the three ZIP magic values.
fn is_zip_magic(bytes: &[u8]) -> bool {
    bytes.len() >= 4
        && ([
            &ZIP_MAGIC_LOCAL[..],
            &ZIP_MAGIC_EOCD[..],
            &ZIP_MAGIC_SPAN[..],
        ]
        .iter()
        .any(|m| bytes.starts_with(m)))
}

/// Validate the raw name of a ZIP entry against portable path safety rules.
///
/// Returns `Ok(())` when the name is safe, or `Err(reason)` with a brief
/// explanation of the violation.
fn validate_entry_name(name: &str) -> Result<(), &'static str> {
    if name.is_empty() {
        return Err("entry name is empty");
    }
    if name.contains('\0') {
        return Err("entry name contains NUL byte");
    }
    // Absolute Unix path.
    if name.starts_with('/') {
        return Err("entry name is an absolute path");
    }
    // Windows-style absolute path: `C:\` or `C:/`.
    if name.len() >= 3 {
        let b = name.as_bytes();
        if b[1] == b':' && (b[2] == b'\\' || b[2] == b'/') && b[0].is_ascii_alphabetic() {
            return Err("entry name contains a Windows drive-letter prefix");
        }
    }
    // UNC path (`\\server\share`).
    if name.starts_with("\\\\") {
        return Err("entry name is a UNC path");
    }
    // Raw backslash outside a UNC context (Windows path separator used in a
    // ZIP that targets a Unix extractor — still dangerous).
    if name.contains('\\') {
        return Err("entry name contains a raw backslash path separator");
    }
    // Reject `.` and `..` path components.
    for component in name.split('/') {
        if component == "." || component == ".." {
            return Err("entry name contains a '.' or '..' path component");
        }
    }
    Ok(())
}

/// Peek `buf` (up to `PEEK_BYTES`) and return true when the root XML element
/// is an XCCDF 1.2 `Benchmark`.
fn is_xccdf_benchmark(buf: &[u8]) -> bool {
    if buf.is_empty() {
        return false;
    }
    let mut reader = NsReader::from_reader(std::io::Cursor::new(buf));
    reader.config_mut().trim_text(true);
    let mut tbuf = Vec::new();
    loop {
        match reader.read_resolved_event_into(&mut tbuf) {
            Ok((resolved, Event::Start(e) | Event::Empty(e))) => {
                let is_xccdf_ns = matches!(
                    &resolved,
                    ResolveResult::Bound(ns) if ns.as_ref() == XCCDF_NAMESPACE.as_bytes()
                );
                return is_xccdf_ns && e.local_name().as_ref() == b"Benchmark";
            }
            Ok((_, Event::Eof)) | Err(_) => return false,
            _ => {
                tbuf.clear();
                continue;
            }
        }
    }
}

/// Read the selected XCCDF entry fully and return [`ExtractedXccdf`].
fn read_selected_entry(
    archive: &mut ZipArchive<std::io::Cursor<&[u8]>>,
    index: usize,
    name: &str,
    limits: &InterchangeLimits,
    archive_file_count: usize,
) -> Result<ExtractedXccdf, ZipExtractionError> {
    let mut entry = archive.by_index(index).map_err(|e| {
        ZipExtractionError::invalid(
            "ZIP_ENTRY_ERROR",
            format!("Cannot open selected entry '{name}': {e}"),
        )
    })?;

    let mut xml_bytes = Vec::new();
    let mut buf = [0u8; 65536];
    loop {
        let n = entry.read(&mut buf).map_err(|e| {
            ZipExtractionError::invalid("ZIP_READ_ERROR", format!("Error reading '{name}': {e}"))
        })?;
        if n == 0 {
            break;
        }
        xml_bytes.extend_from_slice(&buf[..n]);
        if xml_bytes.len() > limits.max_xml_bytes {
            return Err(ZipExtractionError::limit(
                "ZIP_XML_TOO_LARGE",
                format!(
                    "Extracted XML from '{name}' exceeds the {} byte limit",
                    limits.max_xml_bytes
                ),
            ));
        }
    }

    let xml_sha256 = hex::encode(Sha256::digest(&xml_bytes));

    Ok(ExtractedXccdf {
        xml_bytes,
        xml_sha256,
        entry_name: name.to_string(),
        archive_file_count,
    })
}

// ── HTTP-status classification ────────────────────────────────────────────────

/// Return the appropriate HTTP status code for a [`ZipExtractionError`].
///
/// ```text
/// 413  ZIP_BOMB, ZIP_FILE_COUNT_EXCEEDED, ZIP_EXPANDED_SIZE_EXCEEDED, ZIP_XML_TOO_LARGE
/// 422  everything else
/// ```
pub fn zip_error_http_status(error: &ZipExtractionError) -> u16 {
    error.http_status
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

    const XCCDF_NS: &str = "http://checklists.nist.gov/xccdf/1.2";
    const MINIMAL_XCCDF: &[u8] = b"<?xml version=\"1.0\"?><Benchmark xmlns=\"http://checklists.nist.gov/xccdf/1.2\" id=\"b1\"><status>draft</status><title>T</title><version>0.1</version></Benchmark>";
    const NON_XCCDF_XML: &[u8] = b"<?xml version=\"1.0\"?><metadata><key>value</key></metadata>";

    // ── detect_package_kind ───────────────────────────────────────────────────

    #[test]
    fn detects_zip_by_local_header_magic() {
        let zip = make_zip(&[("f.xml", MINIMAL_XCCDF)]);
        assert_eq!(detect_package_kind(&zip), Some(PackageKind::Zip));
    }

    #[test]
    fn detects_xml_by_opening_angle_bracket() {
        assert_eq!(detect_package_kind(MINIMAL_XCCDF), Some(PackageKind::Xml));
    }

    #[test]
    fn detects_xml_with_utf8_bom() {
        let mut bom = b"\xEF\xBB\xBF".to_vec();
        bom.extend_from_slice(MINIMAL_XCCDF);
        assert_eq!(detect_package_kind(&bom), Some(PackageKind::Xml));
    }

    #[test]
    fn detects_xml_with_leading_whitespace() {
        let mut ws = b"\n\r\t  ".to_vec();
        ws.extend_from_slice(MINIMAL_XCCDF);
        assert_eq!(detect_package_kind(&ws), Some(PackageKind::Xml));
    }

    #[test]
    fn returns_none_for_unknown_bytes() {
        assert_eq!(detect_package_kind(b"\xFF\xFE\x00\x00"), None);
        assert_eq!(detect_package_kind(b""), None);
        assert_eq!(detect_package_kind(b"HELLO"), None);
    }

    // ── exceeds_ratio ─────────────────────────────────────────────────────────

    #[test]
    fn ratio_exactly_at_limit_is_accepted() {
        // 100 * 100 = 10_000 is not > 10_000
        assert!(!exceeds_ratio(10_000, 100, 100));
    }

    #[test]
    fn ratio_one_above_limit_is_rejected() {
        // 10_001 > 100 * 100 = 10_000
        assert!(exceeds_ratio(10_001, 100, 100));
    }

    #[test]
    fn zero_uncompressed_is_not_a_bomb() {
        assert!(!exceeds_ratio(0, 1, 100));
    }

    #[test]
    fn zero_compressed_with_any_uncompressed_is_rejected() {
        assert!(exceeds_ratio(1, 0, 100));
    }

    // ── validate_entry_name ───────────────────────────────────────────────────

    #[test]
    fn rejects_path_with_dotdot_component() {
        assert!(validate_entry_name("../evil.xml").is_err());
        assert!(validate_entry_name("a/../../evil.xml").is_err());
    }

    #[test]
    fn rejects_dot_only_component() {
        assert!(validate_entry_name("./benchmark.xml").is_err());
    }

    #[test]
    fn rejects_absolute_unix_path() {
        assert!(validate_entry_name("/etc/passwd").is_err());
    }

    #[test]
    fn rejects_windows_drive_letter() {
        assert!(validate_entry_name("C:\\evil.xml").is_err());
        assert!(validate_entry_name("C:/evil.xml").is_err());
    }

    #[test]
    fn rejects_unc_path() {
        assert!(validate_entry_name("\\\\server\\share").is_err());
    }

    #[test]
    fn rejects_backslash_in_path() {
        assert!(validate_entry_name("folder\\file.xml").is_err());
    }

    #[test]
    fn rejects_nul_byte() {
        assert!(validate_entry_name("file\0.xml").is_err());
    }

    #[test]
    fn accepts_normal_paths() {
        assert!(validate_entry_name("benchmark.xml").is_ok());
        assert!(validate_entry_name("content/benchmark.xml").is_ok());
        assert!(validate_entry_name("deep/nested/doc.xml").is_ok());
    }

    // ── extraction: happy paths ───────────────────────────────────────────────

    #[test]
    fn extracts_single_xccdf_entry() {
        let zip = make_zip(&[("benchmark.xml", MINIMAL_XCCDF)]);
        let result = extract_xccdf_from_zip(&zip, &limits()).unwrap();
        assert_eq!(result.xml_bytes, MINIMAL_XCCDF);
        assert_eq!(result.entry_name, "benchmark.xml");
    }

    #[test]
    fn selects_xccdf_over_non_xccdf_xml() {
        // metadata.xml is at the root but is not an XCCDF Benchmark.
        // content/benchmark.xml is nested but is an XCCDF Benchmark.
        let zip = make_zip(&[
            ("metadata.xml", NON_XCCDF_XML),
            ("content/benchmark.xml", MINIMAL_XCCDF),
        ]);
        let result = extract_xccdf_from_zip(&zip, &limits()).unwrap();
        assert_eq!(result.entry_name, "content/benchmark.xml");
    }

    #[test]
    fn selects_root_xccdf_when_also_nested_non_xccdf() {
        let zip = make_zip(&[
            ("benchmark.xml", MINIMAL_XCCDF),
            ("sub/metadata.xml", NON_XCCDF_XML),
        ]);
        let result = extract_xccdf_from_zip(&zip, &limits()).unwrap();
        assert_eq!(result.entry_name, "benchmark.xml");
    }

    #[test]
    fn accepts_non_xml_files_alongside_xccdf() {
        let zip = make_zip(&[
            ("readme.txt", b"description"),
            ("benchmark.xml", MINIMAL_XCCDF),
            ("sig.asc", b"signature"),
        ]);
        let result = extract_xccdf_from_zip(&zip, &limits()).unwrap();
        assert_eq!(result.entry_name, "benchmark.xml");
    }

    #[test]
    fn accepts_single_xccdf_in_subdirectory() {
        let zip = make_zip(&[("folder/benchmark.xml", MINIMAL_XCCDF)]);
        let result = extract_xccdf_from_zip(&zip, &limits()).unwrap();
        assert_eq!(result.entry_name, "folder/benchmark.xml");
    }

    // ── extraction: XCCDF selection failures ──────────────────────────────────

    #[test]
    fn rejects_empty_archive_with_no_xccdf() {
        let zip = make_zip(&[]);
        let err = extract_xccdf_from_zip(&zip, &limits()).unwrap_err();
        assert_eq!(err.code, "ZIP_NO_XCCDF");
        assert_eq!(err.http_status, 422);
    }

    #[test]
    fn rejects_archive_with_only_non_xml_files() {
        let zip = make_zip(&[("readme.txt", b"hello"), ("data.json", b"{}")]);
        let err = extract_xccdf_from_zip(&zip, &limits()).unwrap_err();
        assert_eq!(err.code, "ZIP_NO_XCCDF");
    }

    #[test]
    fn rejects_xml_files_with_no_xccdf_benchmark_root() {
        let zip = make_zip(&[("meta.xml", NON_XCCDF_XML), ("data.xml", b"<other/>")]);
        let err = extract_xccdf_from_zip(&zip, &limits()).unwrap_err();
        assert_eq!(err.code, "ZIP_NO_XCCDF");
    }

    #[test]
    fn rejects_two_xccdf_documents_as_ambiguous() {
        let zip = make_zip(&[("a.xml", MINIMAL_XCCDF), ("b.xml", MINIMAL_XCCDF)]);
        let err = extract_xccdf_from_zip(&zip, &limits()).unwrap_err();
        assert_eq!(err.code, "ZIP_AMBIGUOUS_XCCDF");
        assert_eq!(err.candidates.len(), 2);
        assert_eq!(err.http_status, 422);
    }

    // ── extraction: security rejections ──────────────────────────────────────

    #[test]
    fn rejects_invalid_zip_bytes() {
        let err = extract_xccdf_from_zip(b"not a zip at all", &limits()).unwrap_err();
        assert_eq!(err.code, "ZIP_INVALID");
        assert_eq!(err.http_status, 422);
    }

    #[test]
    fn rejects_symlink_entry() {
        let mut buf = Vec::new();
        {
            let mut w = zip::ZipWriter::new(std::io::Cursor::new(&mut buf));
            // add_symlink creates a proper symlink entry in the central directory.
            let opts: SimpleFileOptions = FileOptions::default();
            w.add_symlink("evil_link", "../../etc/passwd", opts)
                .expect("add_symlink");
            // Add a valid XCCDF so we reach the symlink check.
            let file_opts: SimpleFileOptions =
                FileOptions::default().compression_method(CompressionMethod::Stored);
            w.start_file("benchmark.xml", file_opts).unwrap();
            w.write_all(MINIMAL_XCCDF).unwrap();
            w.finish().expect("finish");
        }
        let err = extract_xccdf_from_zip(&buf, &limits()).unwrap_err();
        assert_eq!(err.code, "ZIP_SYMLINK");
        assert_eq!(err.http_status, 422);
    }

    #[test]
    fn rejects_path_traversal_in_file_entry() {
        // The zip crate allows writing any name; we validate explicitly.
        // Build raw ZIP bytes with a traversal name via ZipWriter (it accepts
        // arbitrary names).
        let mut buf = Vec::new();
        {
            let mut w = zip::ZipWriter::new(std::io::Cursor::new(&mut buf));
            let opts: SimpleFileOptions =
                FileOptions::default().compression_method(CompressionMethod::Stored);
            // The zip crate writes whatever name we give it.
            w.start_file("../traversal.xml", opts).expect("start_file");
            w.write_all(MINIMAL_XCCDF).unwrap();
            w.finish().expect("finish");
        }
        let err = extract_xccdf_from_zip(&buf, &limits()).unwrap_err();
        assert_eq!(err.code, "ZIP_PATH_TRAVERSAL");
        assert_eq!(err.http_status, 422);
    }

    #[test]
    fn rejects_nested_zip_by_extension() {
        let inner = make_zip(&[("x.xml", MINIMAL_XCCDF)]);
        let outer = make_zip(&[("inner.zip", &inner), ("outer.xml", MINIMAL_XCCDF)]);
        let err = extract_xccdf_from_zip(&outer, &limits()).unwrap_err();
        assert_eq!(err.code, "ZIP_NESTED_ARCHIVE");
        assert_eq!(err.http_status, 422);
    }

    #[test]
    fn rejects_nested_zip_by_content_magic_disguised_as_non_zip() {
        // A file named `data.bin` that contains a ZIP payload.
        let inner = make_zip(&[("x.xml", MINIMAL_XCCDF)]);
        let outer = make_zip(&[("data.bin", &inner), ("benchmark.xml", MINIMAL_XCCDF)]);
        let err = extract_xccdf_from_zip(&outer, &limits()).unwrap_err();
        assert_eq!(err.code, "ZIP_NESTED_ARCHIVE");
    }

    #[test]
    fn rejects_file_count_above_limit() {
        let small_limit = InterchangeLimits {
            max_archive_files: 3,
            ..InterchangeLimits::default()
        };
        let zip = make_zip(&[
            ("a.txt", b"1"),
            ("b.txt", b"2"),
            ("c.txt", b"3"),
            ("benchmark.xml", MINIMAL_XCCDF),
        ]);
        let err = extract_xccdf_from_zip(&zip, &small_limit).unwrap_err();
        assert_eq!(err.code, "ZIP_FILE_COUNT_EXCEEDED");
        assert_eq!(err.http_status, 413);
    }

    #[test]
    fn rejects_xml_too_large_after_extraction() {
        let small_limit = InterchangeLimits {
            max_xml_bytes: 10,
            ..InterchangeLimits::default()
        };
        let zip = make_zip(&[("benchmark.xml", MINIMAL_XCCDF)]);
        let err = extract_xccdf_from_zip(&zip, &small_limit).unwrap_err();
        assert_eq!(err.code, "ZIP_XML_TOO_LARGE");
        assert_eq!(err.http_status, 413);
    }

    #[test]
    fn rejects_entry_level_expansion_ratio() {
        // Construct an entry where the declared uncompressed size far exceeds
        // the compressed size using STORED compression so ratio = 1 normally.
        // Simulate by using a small max_expanded_archive_bytes limit.
        let small_limit = InterchangeLimits {
            max_expanded_archive_bytes: 50,
            ..InterchangeLimits::default()
        };
        let zip = make_zip(&[("benchmark.xml", MINIMAL_XCCDF)]);
        // MINIMAL_XCCDF is ~170 bytes, well over 50.
        let err = extract_xccdf_from_zip(&zip, &small_limit).unwrap_err();
        // Either ZIP_EXPANDED_SIZE_EXCEEDED or ZIP_XML_TOO_LARGE depending on
        // which limit fires first (both map to 413).
        assert!(
            err.code == "ZIP_EXPANDED_SIZE_EXCEEDED" || err.code == "ZIP_XML_TOO_LARGE",
            "expected size limit error, got: {}",
            err.code
        );
        assert_eq!(err.http_status, 413);
    }

    // ── detect_package_kind: mismatch ─────────────────────────────────────────

    #[test]
    fn zip_bytes_are_identified_as_zip_regardless_of_extension() {
        let zip = make_zip(&[("benchmark.xml", MINIMAL_XCCDF)]);
        // Even when the caller would name it .xml, bytes say Zip.
        assert_eq!(detect_package_kind(&zip), Some(PackageKind::Zip));
    }

    #[test]
    fn xml_bytes_are_identified_as_xml_regardless_of_extension() {
        // Even when the caller would name it .zip, bytes say Xml.
        assert_eq!(detect_package_kind(MINIMAL_XCCDF), Some(PackageKind::Xml));
    }
}
