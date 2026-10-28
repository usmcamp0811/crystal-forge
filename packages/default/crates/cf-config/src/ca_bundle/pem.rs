//! Decodes bounded PEM and buildcatrust metadata without selecting roots.
//!
//! CA callers must run the raw-line guard before whitespace normalization.
//! Client identity parsing deliberately retains its separate framing contract.

use super::CaBundleError;
use base64::{Engine, engine::general_purpose::STANDARD};

// Resource bounds permit large system bundles while bounding allocations and
// OpenSSL work. Labels are independently limited to 256 Unicode characters.
/// Bounds the original bundle size in bytes before parser allocation.
pub(super) const MAX_BYTES: usize = 4 * 1024 * 1024;
const MAX_LINE: usize = 4096;
const MAX_BLOCK: usize = 128 * 1024;
/// Bounds certificate records, including records that do not supply roots.
pub(super) const MAX_RECORDS: usize = 8192;
/// Identifies the generator's exact replacement for an omitted ordinary root.
pub(super) const OMITTED: &str =
    "Traditional PEM block omitted: this certificate is not trusted for authenticating servers.";
/// Maps supported EKU suffixes 1 through 8 to exact generator descriptions.
pub(super) const PURPOSES: [&str; 8] = [
    "RFC5280: serverAuth key usage",
    "RFC5280: clientAuth key usage",
    "RFC5280: codeSigning key usage",
    "RFC5280: emailProtection key usage",
    "RFC2459: ipsecEndSystem key usage",
    "RFC2459: ipsecTunnel key usage",
    "RFC2459: ipsecUser key usage",
    "RFC5280: timeStamping key usage",
];

/// Checks original CA lines before any trimming can promote a Go-ignored root.
///
/// # Errors
/// Returns `TooLarge` for oversized input or `InvalidFraming` for controls and
/// indented delimiters. Only LF and CRLF terminate CA lines.
pub(super) fn validate_ca_lines(input: &str) -> Result<(), CaBundleError> {
    if input.len() > MAX_BYTES {
        return Err(CaBundleError::TooLarge);
    }
    // SECURITY: Go's pem.Decode requires delimiters in column zero. Check raw
    // CA lines before the shared parser trims whitespace; otherwise normalization
    // could promote a certificate that Go ignored into the explicit root set.
    // Only LF or CRLF terminate lines. Never strip an EOF CR or VT/FF from PEM
    // bodies or delimiter lines; Go does not treat VT/FF as PEM whitespace.
    // This CA-only guard does not change the separate client identity contract.
    for raw in input.split_inclusive('\n') {
        let line = match raw.strip_suffix('\n') {
            Some(line) => line.strip_suffix('\r').unwrap_or(line),
            None => raw,
        };
        if line.chars().any(|c| c.is_control() && c != '\t') {
            return Err(CaBundleError::InvalidFraming);
        }
        let unindented = line.trim_start_matches([' ', '\t']);
        if unindented.len() != line.len()
            && (unindented.starts_with("-----BEGIN ") || unindented.starts_with("-----END "))
        {
            return Err(CaBundleError::InvalidFraming);
        }
    }
    Ok(())
}

/// Tracks bounded source lines and the next unconsumed line.
///
/// CA parsing skips safe comments; identity parsing inspects raw lines instead.
/// Consumers must advance `position` only across complete consumed lines.
pub(super) struct Parser<'a> {
    /// Retains untrimmed lines so identity fields can forbid comments.
    pub(super) lines: Vec<&'a str>,
    /// Identifies the next unconsumed line; never exceeds `lines.len()`.
    pub(super) position: usize,
}

impl<'a> Parser<'a> {
    /// Creates a line cursor without applying CA-specific delimiter rules.
    ///
    /// # Errors
    /// Returns `TooLarge` for bundle/line limits or `UnsupportedContent` for
    /// controls other than tabs after removing one trailing CR per split line.
    pub(super) fn new(input: &'a str) -> Result<Self, CaBundleError> {
        if input.len() > MAX_BYTES {
            return Err(CaBundleError::TooLarge);
        }
        let mut lines = Vec::new();
        for line in input.split('\n') {
            let line = line.strip_suffix('\r').unwrap_or(line);
            if line.len() > MAX_LINE {
                return Err(CaBundleError::TooLarge);
            }
            if line.chars().any(|c| c.is_control() && c != '\t') {
                return Err(CaBundleError::UnsupportedContent);
            }
            lines.push(line);
        }
        Ok(Self { lines, position: 0 })
    }

    /// Returns the next space/tab-trimmed nonblank, noncomment line.
    ///
    /// # Errors
    /// Returns `UnsupportedContent` for comments with PEM markers or key headers.
    pub(super) fn next_content(&mut self) -> Result<Option<&'a str>, CaBundleError> {
        while let Some(line) = self.lines.get(self.position) {
            self.position += 1;
            let line = line.trim_matches([' ', '\t']);
            if line.is_empty() {
                continue;
            }
            if line.starts_with('#') {
                if line.contains("-----")
                    || line.contains("PRIVATE KEY")
                    || line.contains("Proc-Type:")
                    || line.contains("DEK-Info:")
                {
                    return Err(CaBundleError::UnsupportedContent);
                }
                continue;
            }
            return Ok(Some(line));
        }
        Ok(None)
    }

    /// Decodes a bounded PEM body after its BEGIN line has been consumed.
    ///
    /// # Errors
    /// Returns `InvalidFraming` for missing/mismatched delimiters or Base64,
    /// `UnsupportedContent` for body characters, or `TooLarge` for encoded size.
    pub(super) fn pem(&mut self, name: &str) -> Result<Vec<u8>, CaBundleError> {
        let end = format!("-----END {name}-----");
        let mut encoded = String::new();
        while let Some(line) = self.lines.get(self.position) {
            self.position += 1;
            let line = line.trim_matches([' ', '\t']);
            if line == end {
                return STANDARD
                    .decode(encoded)
                    .map_err(|_| CaBundleError::InvalidFraming);
            }
            if line.contains("-----") {
                return Err(CaBundleError::InvalidFraming);
            }
            for byte in line.bytes() {
                if matches!(byte, b' ' | b'\t') {
                    continue;
                }
                if !byte.is_ascii_alphanumeric() && !matches!(byte, b'+' | b'/' | b'=') {
                    return Err(CaBundleError::UnsupportedContent);
                }
                encoded.push(char::from(byte));
            }
            if encoded.len() > MAX_BLOCK {
                return Err(CaBundleError::TooLarge);
            }
        }
        Err(CaBundleError::InvalidFraming)
    }

    /// Reads an optional exact generator EKU list, retaining its order.
    ///
    /// An absent heading leaves the cursor unchanged and returns an empty list.
    ///
    /// # Errors
    /// Returns content/framing errors from the cursor, `UnsupportedContent` for
    /// empty or noncanonical lists, or `AmbiguousTrust` for duplicate purposes.
    pub(super) fn purposes(&mut self, heading: &str) -> Result<Vec<u8>, CaBundleError> {
        let checkpoint = self.position;
        if self.next_content()? != Some(heading) {
            self.position = checkpoint;
            return Ok(Vec::new());
        }
        let mut purposes = Vec::new();
        loop {
            let checkpoint = self.position;
            let Some(line) = self.next_content()? else {
                break;
            };
            if !line.starts_with("- ") {
                self.position = checkpoint;
                break;
            }
            // The public generator emits exactly two spaces before each dash.
            if self.lines[self.position - 1] != format!("  {line}") {
                return Err(CaBundleError::UnsupportedContent);
            }
            let purpose = (1..=8)
                .find(|n| line == format!("- 1.3.6.1.5.5.7.3.{n} ({})", PURPOSES[*n as usize - 1]))
                .ok_or(CaBundleError::UnsupportedContent)?;
            if purposes.contains(&purpose) {
                return Err(CaBundleError::AmbiguousTrust);
            }
            purposes.push(purpose);
        }
        if purposes.is_empty() {
            return Err(CaBundleError::UnsupportedContent);
        }
        Ok(purposes)
    }
}

/// Checks an NSS nickname as metadata, never as authentication evidence.
///
/// # Errors
/// Returns `UnsupportedContent` for overlong labels, controls, reserved grammar,
/// PEM/key markers, URLs, or padding. A complete paired record must follow.
pub(super) fn validate_label(label: &str) -> Result<(), CaBundleError> {
    // NSS nicknames are human-readable aliases, not authentication evidence.
    // A nickname can look like Base64 without being private-key material.
    // Do not decode labels: acceptance depends on the complete paired record's
    // certificate DER, AUX trust data, and matching annotations instead.
    if label.chars().count() > 256
        || label.chars().any(char::is_control)
        || label.contains("-----")
        || label.contains("BEGIN")
        || label.contains("END")
        || label.contains("PRIVATE KEY")
        || label.contains("://")
        || label.contains('=')
        || label.starts_with('-')
        || label == OMITTED
        || label == "Trusted for:"
        || label == "Rejected for:"
    {
        return Err(CaBundleError::UnsupportedContent);
    }
    Ok(())
}

/// Encodes DER with LF delimiters and 64-character Base64 lines.
pub(super) fn encode_pem(name: &str, der: &[u8]) -> String {
    let mut output = format!("-----BEGIN {name}-----\n");
    let encoded = STANDARD.encode(der);
    for chunk in encoded.as_bytes().chunks(64) {
        for byte in chunk {
            output.push(char::from(*byte));
        }
        output.push('\n');
    }
    output.push_str(&format!("-----END {name}-----\n"));
    output
}
