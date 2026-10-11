//! Validates explicit CA bundles without changing their Go-compatible root set.
//!
//! Accepts ordinary certificate PEM and complete buildcatrust v0.5.1 OpenSSL
//! records. Auxiliary certificates are validated but never converted into roots.
//! No ambient trust is added. Client identity validation is a separate contract.

use base64::{Engine, engine::general_purpose::STANDARD};
use openssl::{pkey::PKey, x509::X509};
use std::fmt;

/// Identifies a credential-safe CA validation failure without retaining input.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CaBundleError {
    /// PEM or DER framing is incomplete, mismatched, or noncanonical.
    InvalidFraming,
    /// A certificate is not a fully consumed canonical X.509 object.
    InvalidX509,
    /// Content is outside the supported PEM or buildcatrust grammar.
    UnsupportedContent,
    /// Certificate selection and auxiliary trust metadata disagree.
    AmbiguousTrust,
    /// No ordinary certificates are available to Go's certificate pool.
    NoCertificates,
    /// Input exceeds the parser's bounded resource limits.
    TooLarge,
}

impl CaBundleError {
    /// Returns a stable static diagnostic code suitable for server/agent errors.
    pub fn code(self) -> &'static str {
        match self {
            Self::InvalidFraming => "ca_bundle_framing_invalid",
            Self::InvalidX509 => "ca_certificate_invalid",
            Self::UnsupportedContent => "ca_bundle_content_unsupported",
            Self::AmbiguousTrust => "ca_bundle_trust_ambiguous",
            Self::NoCertificates => "ca_bundle_empty",
            Self::TooLarge => "ca_bundle_too_large",
        }
    }
}

impl fmt::Display for CaBundleError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.code())
    }
}
impl std::error::Error for CaBundleError {}

// Resource bounds permit large system bundles while bounding allocations and
// OpenSSL work. Labels are independently limited to 256 Unicode characters.
const MAX_BYTES: usize = 4 * 1024 * 1024;
const MAX_LINE: usize = 4096;
const MAX_BLOCK: usize = 128 * 1024;
const MAX_RECORDS: usize = 8192;
const OMITTED: &str =
    "Traditional PEM block omitted: this certificate is not trusted for authenticating servers.";
const PURPOSES: [&str; 8] = [
    "RFC5280: serverAuth key usage",
    "RFC5280: clientAuth key usage",
    "RFC5280: codeSigning key usage",
    "RFC5280: emailProtection key usage",
    "RFC2459: ipsecEndSystem key usage",
    "RFC2459: ipsecTunnel key usage",
    "RFC2459: ipsecUser key usage",
    "RFC5280: timeStamping key usage",
];

/// Returns canonical certificate-only PEM containing every ordinary input root.
///
/// Output uses LF and 64-character Base64 lines. Order and duplicates are retained.
/// Complete buildcatrust records require a bounded label, a plain certificate or
/// the exact omission note, exact optional EKU lists, and a matching trusted
/// certificate with fully consumed AUX DER. Standalone trusted blocks are also
/// validated and discarded. Their presence never adds a root. Spaces, tabs and
/// LF or CRLF line endings, including whitespace-only lines, and
/// complete `#` comment lines are allowed outside PEM bodies; comments containing
/// PEM markers or private-key headers are rejected. Inline Base64 comments fail.
/// PEM BEGIN and END delimiters must start in column zero. Trailing spaces and
/// tabs and spaces/tabs within Base64 bodies are accepted. Bare CR, vertical tab,
/// form feed and all other control characters are rejected throughout CA input.
/// A final END delimiter need not end with a newline. Callers must pass the
/// original CA text without trimming leading whitespace: trimming can make a
/// certificate ignored by Go eligible as a root.
///
/// The grammar follows buildcatrust's `certstore_output.py`, `der_x509.py`, and
/// `x509_consts.py` at v0.5.1. AUX alias, key ID, other algorithms and unknown
/// purposes are unsupported. Labels are not compared with subject CN: NSS
/// nicknames can differ from certificate subjects. Base64-shaped nicknames are
/// accepted only within a complete validated buildcatrust record.
/// Limits are 4 MiB per bundle, 4096 bytes per line, 128 KiB of encoded Base64
/// per block, 8192 records, and 256 characters per label.
///
/// # Errors
/// Returns [`CaBundleError`] for unknown content, invalid framing/X.509, trust
/// contradictions, resource limits, or a bundle with no ordinary certificates.
/// Errors and their debug representations contain no input or OpenSSL details.
///
/// # Examples
/// ```
/// use cf_config::ca_bundle::{normalize_ca_bundle, CaBundleError};
/// assert_eq!(normalize_ca_bundle("# empty bundle\n"), Err(CaBundleError::NoCertificates));
/// ```
pub fn normalize_ca_bundle(input: &str) -> Result<String, CaBundleError> {
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
    let mut parser = Parser::new(input)?;
    let mut roots = Vec::new();
    let mut rejected = Vec::new();
    let mut records = 0;
    while let Some(line) = parser.next_content()? {
        records += 1;
        if records > MAX_RECORDS {
            return Err(CaBundleError::TooLarge);
        }
        if line == "-----BEGIN CERTIFICATE-----" {
            let der = parser.pem("CERTIFICATE")?;
            certificate(&der)?;
            roots.push(der);
        } else if line == "-----BEGIN TRUSTED CERTIFICATE-----" {
            let (der, trust, reject) = trusted(&parser.pem("TRUSTED CERTIFICATE")?)?;
            check_overlap(&trust, &reject)?;
            if reject.contains(&1) {
                rejected.push(der);
            }
        } else {
            validate_label(line)?;
            let plain = match parser.next_content()? {
                Some("-----BEGIN CERTIFICATE-----") => {
                    let der = parser.pem("CERTIFICATE")?;
                    certificate(&der)?;
                    Some(der)
                }
                Some(OMITTED) => None,
                _ => return Err(CaBundleError::UnsupportedContent),
            };
            let text_trust = parser.purposes("Trusted for:")?;
            let text_reject = parser.purposes("Rejected for:")?;
            if parser.next_content()? != Some("-----BEGIN TRUSTED CERTIFICATE-----") {
                return Err(CaBundleError::UnsupportedContent);
            }
            let (der, trust, reject) = trusted(&parser.pem("TRUSTED CERTIFICATE")?)?;
            check_overlap(&trust, &reject)?;
            if text_trust != trust
                || text_reject != reject
                || plain.is_some() != trust.contains(&1)
                || plain.as_ref().is_some_and(|cert| cert != &der)
            {
                return Err(CaBundleError::AmbiguousTrust);
            }
            if reject.contains(&1) {
                rejected.push(der);
            }
            if let Some(plain) = plain {
                roots.push(plain);
            }
        }
    }
    // INVARIANT: An explicit serverAuth rejection cannot be hidden by another
    // ordinary copy of the same certificate elsewhere in the bundle.
    if roots.iter().any(|root| rejected.contains(root)) {
        return Err(CaBundleError::AmbiguousTrust);
    }
    if roots.is_empty() {
        return Err(CaBundleError::NoCertificates);
    }
    let mut output = String::new();
    for root in roots {
        output.push_str(&encode_pem("CERTIFICATE", &root));
    }
    Ok(output)
}

struct Parser<'a> {
    lines: Vec<&'a str>,
    position: usize,
}

impl<'a> Parser<'a> {
    fn new(input: &'a str) -> Result<Self, CaBundleError> {
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

    fn next_content(&mut self) -> Result<Option<&'a str>, CaBundleError> {
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

    fn pem(&mut self, name: &str) -> Result<Vec<u8>, CaBundleError> {
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

    fn purposes(&mut self, heading: &str) -> Result<Vec<u8>, CaBundleError> {
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

fn validate_label(label: &str) -> Result<(), CaBundleError> {
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

// Parses only the few canonical TLVs needed for certificate/AUX boundaries.
// Indefinite, overlong and overflowing lengths are forbidden by DER.
fn tlv<'a>(input: &mut &'a [u8], tag: u8) -> Result<&'a [u8], CaBundleError> {
    if input.len() < 2 || input[0] != tag {
        return Err(CaBundleError::InvalidFraming);
    }
    let first = input[1];
    let (header, length) = if first < 128 {
        (2, usize::from(first))
    } else {
        let count = usize::from(first & 127);
        if count == 0
            || count > std::mem::size_of::<usize>()
            || input.len() < 2 + count
            || input[2] == 0
        {
            return Err(CaBundleError::InvalidFraming);
        }
        let mut length = 0usize;
        for byte in &input[2..2 + count] {
            length = length
                .checked_mul(256)
                .and_then(|n| n.checked_add(usize::from(*byte)))
                .ok_or(CaBundleError::InvalidFraming)?;
        }
        if length < 128 {
            return Err(CaBundleError::InvalidFraming);
        }
        (2 + count, length)
    };
    let end = header
        .checked_add(length)
        .ok_or(CaBundleError::InvalidFraming)?;
    if end > input.len() {
        return Err(CaBundleError::InvalidFraming);
    }
    let contents = &input[header..end];
    *input = &input[end..];
    Ok(contents)
}

fn certificate(der: &[u8]) -> Result<X509, CaBundleError> {
    let mut remaining = der;
    tlv(&mut remaining, 0x30).map_err(|_| CaBundleError::InvalidX509)?;
    if !remaining.is_empty() {
        return Err(CaBundleError::InvalidX509);
    }
    let cert = X509::from_der(der).map_err(|_| CaBundleError::InvalidX509)?;
    if cert.to_der().map_err(|_| CaBundleError::InvalidX509)? != der {
        return Err(CaBundleError::InvalidX509);
    }
    Ok(cert)
}

fn oid_list(mut input: &[u8]) -> Result<Vec<u8>, CaBundleError> {
    let mut result = Vec::new();
    while !input.is_empty() {
        let oid = tlv(&mut input, 0x06)?;
        if oid.len() != 8 || oid[..7] != [43, 6, 1, 5, 5, 7, 3] || !(1..=8).contains(&oid[7]) {
            return Err(CaBundleError::UnsupportedContent);
        }
        if result.contains(&oid[7]) {
            return Err(CaBundleError::AmbiguousTrust);
        }
        result.push(oid[7]);
    }
    Ok(result)
}

fn trusted(input: &[u8]) -> Result<(Vec<u8>, Vec<u8>, Vec<u8>), CaBundleError> {
    let mut remaining = input;
    tlv(&mut remaining, 0x30).map_err(|_| CaBundleError::InvalidX509)?;
    let der = input[..input.len() - remaining.len()].to_vec();
    certificate(&der)?;
    let mut aux = tlv(&mut remaining, 0x30)?;
    if !remaining.is_empty() {
        return Err(CaBundleError::InvalidFraming);
    }
    let trust = oid_list(tlv(&mut aux, 0x30)?)?;
    let reject = if aux.first() == Some(&0xa0) {
        oid_list(tlv(&mut aux, 0xa0)?)?
    } else {
        Vec::new()
    };
    if !aux.is_empty() {
        return Err(CaBundleError::UnsupportedContent);
    }
    Ok((der, trust, reject))
}

fn check_overlap(trust: &[u8], reject: &[u8]) -> Result<(), CaBundleError> {
    if trust.iter().any(|purpose| reject.contains(purpose)) {
        return Err(CaBundleError::AmbiguousTrust);
    }
    Ok(())
}

fn encode_pem(name: &str, der: &[u8]) -> String {
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

/// Identifies a safe client-chain, key-format, or identity-pair failure.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClientIdentityError {
    /// The chain is not strict certificate-only canonical X.509 PEM.
    InvalidCertificate,
    /// The key is not exactly one supported unencrypted private-key PEM object.
    InvalidPrivateKey,
    /// The private key does not match the first certificate's public key.
    KeyMismatch,
}
impl ClientIdentityError {
    /// Returns a stable static diagnostic code without cryptographic input.
    pub fn code(self) -> &'static str {
        match self {
            Self::InvalidCertificate => "client_certificate_invalid",
            Self::InvalidPrivateKey => "client_private_key_invalid",
            Self::KeyMismatch => "client_identity_mismatch",
        }
    }
}
impl fmt::Display for ClientIdentityError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.code())
    }
}
impl std::error::Error for ClientIdentityError {}

/// Validates a strict certificate-only chain and its first certificate's key.
///
/// Accepts exactly one unencrypted PKCS#8, RSA, or EC private-key PEM block.
/// Validates all chain certificate structures without adding trust or checking
/// validity dates, issuer signatures, remote authorization, or certificate EKUs.
/// CA annotations, trusted AUX blocks and additional private keys are forbidden.
/// This function does not write files or return private material.
///
/// # Errors
/// Returns a static [`ClientIdentityError`] for certificate/key framing or
/// structure failures, encrypted/unsupported keys, or a public-key mismatch.
///
/// # Examples
/// ```
/// use cf_config::ca_bundle::{validate_client_identity, ClientIdentityError};
/// assert_eq!(validate_client_identity("", ""), Err(ClientIdentityError::InvalidCertificate));
/// ```
pub fn validate_client_identity(cert: &str, key: &str) -> Result<(), ClientIdentityError> {
    use ClientIdentityError::*;
    let mut parser = Parser::new(cert).map_err(|_| InvalidCertificate)?;
    let mut first = None;
    let mut count = 0;
    // Strict identity input accepts whitespace only outside certificate blocks.
    while parser.position < parser.lines.len() {
        let line = parser.lines[parser.position].trim_matches([' ', '\t']);
        parser.position += 1;
        if line.is_empty() {
            continue;
        }
        if line != "-----BEGIN CERTIFICATE-----" {
            return Err(InvalidCertificate);
        }
        let cert = certificate(&parser.pem("CERTIFICATE").map_err(|_| InvalidCertificate)?)
            .map_err(|_| InvalidCertificate)?;
        count += 1;
        if count > MAX_RECORDS {
            return Err(InvalidCertificate);
        }
        if first.is_none() {
            first = Some(cert);
        }
    }
    let first = first.ok_or(InvalidCertificate)?;
    let mut parser = Parser::new(key).map_err(|_| InvalidPrivateKey)?;
    let header = parser
        .next_content()
        .map_err(|_| InvalidPrivateKey)?
        .ok_or(InvalidPrivateKey)?;
    // No comments or annotations are permitted in a private identity field.
    if parser.lines[..parser.position - 1]
        .iter()
        .any(|line| !line.trim().is_empty())
    {
        return Err(InvalidPrivateKey);
    }
    let name = match header {
        "-----BEGIN PRIVATE KEY-----" => "PRIVATE KEY",
        "-----BEGIN RSA PRIVATE KEY-----" => "RSA PRIVATE KEY",
        "-----BEGIN EC PRIVATE KEY-----" => "EC PRIVATE KEY",
        _ => return Err(InvalidPrivateKey),
    };
    let der = parser.pem(name).map_err(|_| InvalidPrivateKey)?;
    if parser.lines[parser.position..]
        .iter()
        .any(|line| !line.trim().is_empty())
    {
        return Err(InvalidPrivateKey);
    }
    let mut remaining = der.as_slice();
    tlv(&mut remaining, 0x30).map_err(|_| InvalidPrivateKey)?;
    if !remaining.is_empty() {
        return Err(InvalidPrivateKey);
    }
    let key = PKey::private_key_from_pem(encode_pem(name, &der).as_bytes())
        .map_err(|_| InvalidPrivateKey)?;
    let public = first.public_key().map_err(|_| InvalidCertificate)?;
    if !key.public_eq(&public) {
        return Err(KeyMismatch);
    }
    Ok(())
}

/// Creates a synthetic public certificate for credential-file ownership tests.
#[cfg(test)]
pub(crate) fn test_certificate() -> String {
    tests::identity().0
}

#[cfg(test)]
mod tests {
    use super::*;
    use openssl::{asn1::Asn1Time, hash::MessageDigest, pkey::Private, x509::X509NameBuilder};

    pub(super) fn identity() -> (String, PKey<Private>) {
        let key = PKey::generate_ed25519().unwrap();
        let mut name = X509NameBuilder::new().unwrap();
        name.append_entry_by_text("CN", "Task470 root ca1").unwrap();
        let name = name.build();
        let mut cert = X509::builder().unwrap();
        cert.set_version(2).unwrap();
        cert.set_subject_name(&name).unwrap();
        cert.set_issuer_name(&name).unwrap();
        cert.set_pubkey(&key).unwrap();
        cert.set_not_before(&Asn1Time::days_from_now(0).unwrap())
            .unwrap();
        cert.set_not_after(&Asn1Time::days_from_now(1).unwrap())
            .unwrap();
        cert.sign(&key, MessageDigest::null()).unwrap();
        (
            String::from_utf8(cert.build().to_pem().unwrap()).unwrap(),
            key,
        )
    }

    fn wrap(tag: u8, content: &[u8]) -> Vec<u8> {
        assert!(content.len() < 128);
        let mut result = vec![tag, content.len() as u8];
        result.extend(content);
        result
    }

    fn aux(cert: &str, trust: &[u8], reject: &[u8]) -> String {
        let mut der = X509::from_pem(cert.as_bytes()).unwrap().to_der().unwrap();
        let oids = |purposes: &[u8]| -> Vec<u8> {
            purposes
                .iter()
                .flat_map(|n| [6, 8, 43, 6, 1, 5, 5, 7, 3, *n])
                .collect()
        };
        let mut contents = wrap(0x30, &oids(trust));
        if !reject.is_empty() {
            contents.extend(wrap(0xa0, &oids(reject)));
        }
        der.extend(wrap(0x30, &contents));
        encode_pem("TRUSTED CERTIFICATE", &der)
    }

    fn record(cert: &str, trust: &[u8], reject: &[u8]) -> String {
        let mut result = "Task470 synthetic NSS nickname: root\n".to_owned();
        if trust.contains(&1) {
            result.push_str(cert);
            result.push('\n');
        } else {
            result.push_str(OMITTED);
            result.push('\n');
        }
        for (heading, purposes) in [("Trusted for:", trust), ("Rejected for:", reject)] {
            if !purposes.is_empty() {
                result.push_str(heading);
                result.push('\n');
                for n in purposes {
                    result.push_str(&format!(
                        "  - 1.3.6.1.5.5.7.3.{n} ({})\n",
                        PURPOSES[*n as usize - 1]
                    ));
                }
            }
        }
        result.push_str(&aux(cert, trust, reject));
        result.push('\n');
        result
    }

    #[test]
    fn ordinary_roots_are_preserved_canonical_and_idempotent() {
        let (private_api, _) = identity();
        let (public_s3, _) = identity();
        let expected = format!("{private_api}{public_s3}{private_api}");
        let input = format!("# harmless complete comment\n \t\n{expected}").replace('\n', "\r\n");
        let output = normalize_ca_bundle(&input).unwrap();
        assert_eq!(output, expected);
        assert_eq!(normalize_ca_bundle(&output).unwrap(), output);
        assert!(!output.contains('\r'));
    }

    #[test]
    fn ca_delimiters_reject_indentation_before_root_selection() {
        let (first, _) = identity();
        let (second, _) = identity();
        assert_ne!(first, second);
        for prefix in [" ", "\t", " \t", "\r", "\u{000b}", "\u{000c}"] {
            for marker in ["-----BEGIN CERTIFICATE-----", "-----END CERTIFICATE-----"] {
                let indented = second.replace(marker, &format!("{prefix}{marker}"));
                for input in [indented.clone(), format!("{first}{indented}")] {
                    assert_eq!(
                        normalize_ca_bundle(&input),
                        Err(CaBundleError::InvalidFraming)
                    );
                }
            }
            let paired = record(&second, &[1], &[]);
            for marker in [
                "-----BEGIN CERTIFICATE-----",
                "-----END CERTIFICATE-----",
                "-----BEGIN TRUSTED CERTIFICATE-----",
                "-----END TRUSTED CERTIFICATE-----",
            ] {
                let indented = paired.replace(marker, &format!("{prefix}{marker}"));
                assert_eq!(
                    normalize_ca_bundle(&format!("{first}{indented}")),
                    Err(CaBundleError::InvalidFraming)
                );
            }
        }
    }

    #[test]
    fn ca_column_zero_delimiters_keep_go_compatible_whitespace() {
        let (first, _) = identity();
        let (second, _) = identity();
        let expected = format!("{first}{second}");
        let mut input = " \t\r\n\t \r\n".to_owned();
        for line in expected.lines() {
            if line.starts_with("-----") {
                input.push_str(line);
                input.push_str(" \t\r\n");
            } else {
                input.push_str(" \t");
                for chunk in line.as_bytes().chunks(4) {
                    input.push_str(std::str::from_utf8(chunk).unwrap());
                    input.push_str(" \t");
                }
                input.push_str("\r\n");
            }
        }
        assert_eq!(normalize_ca_bundle(&input).unwrap(), expected);
        assert_eq!(
            normalize_ca_bundle(&record(&first, &[1], &[])).unwrap(),
            first
        );
    }

    #[test]
    fn ca_raw_controls_and_bare_cr_cannot_promote_a_second_root() {
        let (first, _) = identity();
        let (second, _) = identity();
        let (begin, rest) = second.split_once('\n').unwrap();
        let body_line = rest.split_once('\n').unwrap().0;
        for control in [
            "\u{000b}", "\u{000c}", "\0", "\u{001b}", "\u{007f}", "\u{0085}",
        ] {
            let cases = [
                rest.replacen(body_line, &format!("{control}{body_line}"), 1),
                rest.replacen(
                    body_line,
                    &format!("{}{control}{}", &body_line[..4], &body_line[4..]),
                    1,
                ),
                format!("{control}\n{rest}"),
            ];
            for body in cases {
                assert_eq!(
                    normalize_ca_bundle(&format!("{first}{begin}\n{body}")),
                    Err(CaBundleError::InvalidFraming)
                );
            }
            for marker in ["-----BEGIN CERTIFICATE-----", "-----END CERTIFICATE-----"] {
                let malformed = second.replace(marker, &format!("{marker}{control}"));
                assert_eq!(
                    normalize_ca_bundle(&format!("{first}{malformed}")),
                    Err(CaBundleError::InvalidFraming)
                );
            }
        }
        let bare_cr_body = rest.replacen(
            body_line,
            &format!("{}\r{}", &body_line[..4], &body_line[4..]),
            1,
        );
        assert_eq!(
            normalize_ca_bundle(&format!("{first}{begin}\n{bare_cr_body}")),
            Err(CaBundleError::InvalidFraming)
        );
        assert_eq!(
            normalize_ca_bundle(&format!("{first}{}\r", second.trim_end_matches('\n'))),
            Err(CaBundleError::InvalidFraming)
        );
        assert_eq!(
            normalize_ca_bundle(&format!("{first}{} \t", second.trim_end_matches('\n'))).unwrap(),
            format!("{first}{second}")
        );
        assert_eq!(
            normalize_ca_bundle(&format!("{first}{second}")),
            normalize_ca_bundle(&format!("{first}{}", second.trim_end_matches('\n')))
        );
    }

    #[test]
    fn base64_shaped_nss_labels_require_a_complete_validated_paired_record() {
        let (cert, _) = identity();
        let label = "TWCA";
        assert!(STANDARD.decode(label).is_ok());
        let paired =
            record(&cert, &[1], &[]).replacen("Task470 synthetic NSS nickname: root", label, 1);
        assert_eq!(normalize_ca_bundle(&paired).unwrap(), cert);
        assert_eq!(
            normalize_ca_bundle(&paired.replace('\n', "\r\n")).unwrap(),
            cert
        );
        let omitted =
            record(&cert, &[2], &[]).replacen("Task470 synthetic NSS nickname: root", label, 1);
        assert_eq!(
            normalize_ca_bundle(&omitted),
            Err(CaBundleError::NoCertificates)
        );
        for input in [
            format!("{label}\n{cert}"),
            format!("{label}\n{}", record(&cert, &[1], &[])),
            format!("{cert}{label}\n"),
            paired.replace("serverAuth", "clientAuth"),
            paired.replace("-----END TRUSTED CERTIFICATE-----", ""),
        ] {
            assert!(normalize_ca_bundle(&input).is_err());
        }
    }

    #[test]
    fn nss_matrix_preserves_122_plain_and_ignores_172_aux_roots() {
        let (cert, _) = identity();
        let mut input = String::new();
        input.push_str(&cert); // A private API root need not have NSS annotations.
        for _ in 0..121 {
            input.push_str(&record(&cert, &[1, 2, 3, 4, 8, 5, 6, 7], &[]));
        }
        for _ in 0..51 {
            input.push_str(&record(&cert, &[2], &[3]));
        }
        assert_eq!(input.matches("-----BEGIN CERTIFICATE-----").count(), 122);
        assert_eq!(
            input.matches("-----BEGIN TRUSTED CERTIFICATE-----").count(),
            172
        );
        assert_eq!(normalize_ca_bundle(&input).unwrap(), cert.repeat(122));
    }

    #[test]
    fn trusted_only_never_becomes_a_go_root_even_with_server_auth() {
        let (cert, _) = identity();
        for input in [
            aux(&cert, &[1], &[]),
            aux(&cert, &[], &[]),
            record(&cert, &[], &[]),
            record(&cert, &[2], &[1]),
        ] {
            assert_eq!(
                normalize_ca_bundle(&input),
                Err(CaBundleError::NoCertificates)
            );
        }
        assert_eq!(
            normalize_ca_bundle(&format!("{cert}{}", aux(&cert, &[1], &[]))).unwrap(),
            cert
        );
    }

    #[test]
    fn rejects_metadata_certificate_and_trust_contradictions() {
        let (cert, _) = identity();
        let (other, _) = identity();
        for input in [
            record(&cert, &[1], &[1]),
            record(&cert, &[1], &[]).replacen(&cert, &other, 1),
            record(&cert, &[1], &[]).replace("serverAuth", "clientAuth"),
            record(&cert, &[2], &[]).replace(OMITTED, &cert),
            format!("{cert}{}", aux(&cert, &[], &[1])),
        ] {
            assert!(normalize_ca_bundle(&input).is_err());
        }
        assert_eq!(
            normalize_ca_bundle(&format!("{cert}{}", aux(&cert, &[], &[1]))),
            Err(CaBundleError::AmbiguousTrust)
        );
    }

    #[test]
    fn rejects_unknown_text_private_material_and_malformed_framing() {
        let (cert, key) = identity();
        let private = String::from_utf8(key.private_key_to_pem_pkcs8().unwrap()).unwrap();
        for suffix in [
            "unexplained data",
            "YWJjZA==",
            "-----BEGIN PRIVATE KEY-----",
            "#-----BEGIN PRIVATE KEY-----",
            "# PRIVATE KEY",
            "\0",
        ] {
            assert!(normalize_ca_bundle(&format!("{cert}{suffix}\n")).is_err());
        }
        for input in [
            format!("{cert}{private}"),
            cert.replace("END CERTIFICATE", "END TRUSTED CERTIFICATE"),
            cert.replace("-----END CERTIFICATE-----", ""),
            cert.replace('\n', "\n# inline comment\n"),
            "-----BEGIN CERTIFICATE-----\nYWJjZA==\n-----END CERTIFICATE-----".into(),
            format!("bare label\n{cert}"),
            format!("YWJjZA==\n{}", record(&cert, &[1], &[])),
        ] {
            assert!(normalize_ca_bundle(&input).is_err());
        }
        assert_eq!(
            normalize_ca_bundle(&"x".repeat(MAX_BYTES + 1)),
            Err(CaBundleError::TooLarge)
        );
    }

    #[test]
    fn rejects_trailing_der_noncanonical_lengths_and_unsupported_aux() {
        let (cert, _) = identity();
        let der = X509::from_pem(cert.as_bytes()).unwrap().to_der().unwrap();
        let mut trailing = der.clone();
        trailing.extend([0, 1, 2]);
        assert_eq!(
            normalize_ca_bundle(&encode_pem("CERTIFICATE", &trailing)),
            Err(CaBundleError::InvalidX509)
        );
        for suffix in [
            vec![0x30, 2, 0x30, 0],
            vec![0x30, 0x80, 0, 0],
            vec![0x30, 0x81, 2, 0x30, 0],
            vec![0x30, 5, 0x30, 0, 0x0c, 1, b'x'],
        ] {
            let mut input = der.clone();
            input.extend(suffix);
            input.push(0);
            assert!(normalize_ca_bundle(&encode_pem("TRUSTED CERTIFICATE", &input)).is_err());
        }
        let mut alias = der;
        alias.extend([0x30, 5, 0x30, 0, 0x0c, 1, b'x']);
        assert_eq!(
            normalize_ca_bundle(&encode_pem("TRUSTED CERTIFICATE", &alias)),
            Err(CaBundleError::UnsupportedContent)
        );
        let mut indefinite = [0x30, 0x80, 0, 0].as_slice();
        assert!(tlv(&mut indefinite, 0x30).is_err());
    }

    #[test]
    fn errors_are_static_and_do_not_retain_input() {
        for error in [
            CaBundleError::InvalidFraming,
            CaBundleError::InvalidX509,
            CaBundleError::UnsupportedContent,
            CaBundleError::AmbiguousTrust,
            CaBundleError::NoCertificates,
            CaBundleError::TooLarge,
        ] {
            assert!(error.code().starts_with("ca_"));
            assert_eq!(error.to_string(), error.code());
            assert!(!format!("{error:?}").contains("synthetic secret"));
        }
    }

    #[test]
    fn identity_pair_is_strict_separate_and_checks_first_chain_certificate() {
        let (cert, key) = identity();
        let (other, other_key) = identity();
        let key = String::from_utf8(key.private_key_to_pem_pkcs8().unwrap()).unwrap();
        let other_key = String::from_utf8(other_key.private_key_to_pem_pkcs8().unwrap()).unwrap();
        assert_eq!(
            validate_client_identity(&format!("{cert}{other}"), &key),
            Ok(())
        );
        assert_eq!(
            validate_client_identity(&format!("{other}{cert}"), &key),
            Err(ClientIdentityError::KeyMismatch)
        );
        assert_eq!(
            validate_client_identity(&cert, &other_key),
            Err(ClientIdentityError::KeyMismatch)
        );
        for chain in [
            record(&cert, &[1], &[]),
            aux(&cert, &[1], &[]),
            format!("# comment\n{cert}"),
            format!("{cert}garbage"),
            cert.replace("END CERTIFICATE", "END PRIVATE KEY"),
        ] {
            assert_eq!(
                validate_client_identity(&chain, &key),
                Err(ClientIdentityError::InvalidCertificate)
            );
        }
        for invalid in [
            format!("{key}{key}"),
            format!("# comment\n{key}"),
            format!("{key}garbage"),
            key.replace("BEGIN PRIVATE KEY", "BEGIN ENCRYPTED PRIVATE KEY"),
        ] {
            assert_eq!(
                validate_client_identity(&cert, &invalid),
                Err(ClientIdentityError::InvalidPrivateKey)
            );
        }
    }

    #[test]
    fn rsa_and_ec_identity_formats_match_but_encrypted_keys_fail() {
        use openssl::{
            ec::{EcGroup, EcKey},
            nid::Nid,
            rsa::Rsa,
            symm::Cipher,
        };
        let rsa = Rsa::generate(2048).unwrap();
        let ec =
            EcKey::generate(&EcGroup::from_curve_name(Nid::X9_62_PRIME256V1).unwrap()).unwrap();
        for (key, traditional) in [
            (
                PKey::from_rsa(rsa.clone()).unwrap(),
                rsa.private_key_to_pem().unwrap(),
            ),
            (
                PKey::from_ec_key(ec.clone()).unwrap(),
                ec.private_key_to_pem().unwrap(),
            ),
        ] {
            let mut name = X509NameBuilder::new().unwrap();
            name.append_entry_by_text("CN", "Task470 identity format")
                .unwrap();
            let name = name.build();
            let mut cert = X509::builder().unwrap();
            cert.set_version(2).unwrap();
            cert.set_subject_name(&name).unwrap();
            cert.set_issuer_name(&name).unwrap();
            cert.set_pubkey(&key).unwrap();
            cert.set_not_before(&Asn1Time::days_from_now(0).unwrap())
                .unwrap();
            cert.set_not_after(&Asn1Time::days_from_now(1).unwrap())
                .unwrap();
            cert.sign(&key, MessageDigest::sha256()).unwrap();
            let cert = String::from_utf8(cert.build().to_pem().unwrap()).unwrap();
            for pem in [traditional, key.private_key_to_pem_pkcs8().unwrap()] {
                assert_eq!(
                    validate_client_identity(&cert, &String::from_utf8(pem).unwrap()),
                    Ok(())
                );
            }
            let encrypted = key
                .private_key_to_pem_pkcs8_passphrase(
                    Cipher::aes_256_cbc(),
                    b"synthetic-test-passphrase",
                )
                .unwrap();
            assert_eq!(
                validate_client_identity(&cert, &String::from_utf8(encrypted).unwrap()),
                Err(ClientIdentityError::InvalidPrivateKey)
            );
        }
    }
}
