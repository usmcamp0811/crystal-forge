//! Validates canonical X.509 and the supported buildcatrust AUX DER subset.
//!
//! AUX purposes retain their input order for exact metadata comparison. This
//! module validates structures; it does not authorize certificates as roots.

use super::CaBundleError;
use openssl::x509::X509;

/// Holds a canonical certificate and fully consumed supported AUX purposes.
///
/// Purpose values are OID suffixes 1 through 8, without duplicates in each list.
/// Trust/rejection overlap is checked by root selection, not DER parsing.
pub(super) struct TrustedCertificate {
    /// Retains certificate-only DER for equality and explicit rejection checks.
    pub(super) certificate_der: Vec<u8>,
    /// Retains ordered trusted EKU suffixes; serverAuth has suffix 1.
    pub(super) trusted_purposes: Vec<u8>,
    /// Retains ordered rejected EKU suffixes; serverAuth has suffix 1.
    pub(super) rejected_purposes: Vec<u8>,
}

/// Consumes one canonical TLV with the expected tag, returning its contents.
///
/// # Errors
/// Returns `InvalidFraming` for a wrong tag, truncation, indefinite/overlong
/// lengths, or overflow. The cursor advances only after successful validation.
pub(super) fn tlv<'a>(input: &mut &'a [u8], tag: u8) -> Result<&'a [u8], CaBundleError> {
    // Parses only the few canonical TLVs needed for certificate/AUX boundaries.
    // Indefinite, overlong and overflowing lengths are forbidden by DER.
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

/// Returns an X.509 object only if its canonical DER consumes the entire input.
///
/// # Errors
/// Returns `InvalidX509` for framing, decoding, trailing data, or noncanonical
/// re-encoding. OpenSSL error details never escape this boundary.
pub(super) fn certificate(der: &[u8]) -> Result<X509, CaBundleError> {
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
        // SECURITY: Match the full supported EKU OID, not only its last byte.
        // Unknown purposes must fail rather than alter serverAuth selection.
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

/// Parses certificate DER followed by exactly one supported AUX sequence.
///
/// # Errors
/// Returns `InvalidX509` for the certificate, `InvalidFraming` for malformed or
/// trailing DER, `UnsupportedContent` for other AUX fields/purposes, or
/// `AmbiguousTrust` for duplicate purposes. No root eligibility is inferred.
pub(super) fn trusted(input: &[u8]) -> Result<TrustedCertificate, CaBundleError> {
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
    Ok(TrustedCertificate {
        certificate_der: der,
        trusted_purposes: trust,
        rejected_purposes: reject,
    })
}
