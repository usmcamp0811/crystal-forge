//! Selects only ordinary Go-compatible roots after validating all AUX records.
//!
//! Standalone trusted certificates never supply roots. Rejected serverAuth DER
//! invalidates an ordinary copy anywhere in the bundle, regardless of order.

use super::{
    CaBundleError,
    der::{TrustedCertificate, certificate, trusted},
    pem::{MAX_RECORDS, OMITTED, Parser, validate_ca_lines, validate_label},
};

/// Accumulates ordinary roots separately from explicit serverAuth rejections.
///
/// Both collections contain canonical certificate-only DER. Root order and
/// duplicates remain intact; AUX data never implicitly supplies an ordinary root.
#[derive(Default)]
struct RootSelection {
    ordinary_roots: Vec<Vec<u8>>,
    server_auth_rejections: Vec<Vec<u8>>,
}

impl RootSelection {
    fn retain_rejection(&mut self, trusted: TrustedCertificate) {
        if trusted.rejected_purposes.contains(&1) {
            self.server_auth_rejections.push(trusted.certificate_der);
        }
    }

    fn finish(self) -> Result<Vec<Vec<u8>>, CaBundleError> {
        // INVARIANT: An explicit serverAuth rejection cannot be hidden by another
        // ordinary copy of the same certificate elsewhere in the bundle.
        if self
            .ordinary_roots
            .iter()
            .any(|root| self.server_auth_rejections.contains(root))
        {
            return Err(CaBundleError::AmbiguousTrust);
        }
        if self.ordinary_roots.is_empty() {
            return Err(CaBundleError::NoCertificates);
        }
        Ok(self.ordinary_roots)
    }
}

/// Returns validated ordinary root DER in input order, retaining duplicates.
///
/// # Errors
/// Returns CA framing/content/size errors, invalid X.509, contradictory trust,
/// or `NoCertificates` when validated records supply no ordinary roots.
pub(super) fn ordinary_roots(input: &str) -> Result<Vec<Vec<u8>>, CaBundleError> {
    validate_ca_lines(input)?;
    let mut parser = Parser::new(input)?;
    let mut selection = RootSelection::default();
    let mut records = 0;
    while let Some(line) = parser.next_content()? {
        records += 1;
        if records > MAX_RECORDS {
            return Err(CaBundleError::TooLarge);
        }
        if line == "-----BEGIN CERTIFICATE-----" {
            let der = parser.pem("CERTIFICATE")?;
            certificate(&der)?;
            selection.ordinary_roots.push(der);
        } else if line == "-----BEGIN TRUSTED CERTIFICATE-----" {
            let trusted = trusted(&parser.pem("TRUSTED CERTIFICATE")?)?;
            check_overlap(&trusted.trusted_purposes, &trusted.rejected_purposes)?;
            selection.retain_rejection(trusted);
        } else {
            validate_label(line)?;
            paired_record(&mut parser, &mut selection)?;
        }
    }
    selection.finish()
}

// A label is accepted only when the following complete buildcatrust record
// agrees byte-for-byte on certificate DER and in order on both EKU lists.
fn paired_record(
    parser: &mut Parser<'_>,
    selection: &mut RootSelection,
) -> Result<(), CaBundleError> {
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
    let trusted = trusted(&parser.pem("TRUSTED CERTIFICATE")?)?;
    check_overlap(&trusted.trusted_purposes, &trusted.rejected_purposes)?;
    if text_trust != trusted.trusted_purposes
        || text_reject != trusted.rejected_purposes
        || plain.is_some() != trusted.trusted_purposes.contains(&1)
        || plain
            .as_ref()
            .is_some_and(|cert| cert != &trusted.certificate_der)
    {
        return Err(CaBundleError::AmbiguousTrust);
    }
    selection.retain_rejection(trusted);
    if let Some(plain) = plain {
        selection.ordinary_roots.push(plain);
    }
    Ok(())
}

fn check_overlap(trust: &[u8], reject: &[u8]) -> Result<(), CaBundleError> {
    if trust.iter().any(|purpose| reject.contains(purpose)) {
        return Err(CaBundleError::AmbiguousTrust);
    }
    Ok(())
}
