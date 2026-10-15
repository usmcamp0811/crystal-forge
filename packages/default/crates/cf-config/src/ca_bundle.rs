//! Validates explicit CA bundles without changing their Go-compatible root set.
//!
//! Accepts ordinary certificate PEM and complete buildcatrust v0.5.1 OpenSSL
//! records. Auxiliary certificates are validated but never converted into roots.
//! No ambient trust is added. Client identity validation is a separate contract.

use openssl::pkey::PKey;
use std::fmt;

mod der;
mod pem;
mod roots;

use der::{certificate, tlv};
use pem::{MAX_RECORDS, Parser, encode_pem};

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
    let roots = roots::ordinary_roots(input)?;
    let mut output = String::new();
    for root in roots {
        output.push_str(&encode_pem("CERTIFICATE", &root));
    }
    Ok(output)
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
mod tests;
