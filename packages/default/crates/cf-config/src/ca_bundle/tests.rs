//! Supplies synthetic certificates and exact buildcatrust records for CA tests.
//!
//! Fixtures contain no host credentials. Test groups cover raw PEM grammar,
//! root selection/AUX contradictions, and the separate client identity contract.

use super::pem::{MAX_BYTES, OMITTED, PURPOSES};
use super::*;
use openssl::{
    asn1::Asn1Time, hash::MessageDigest, pkey::Private, x509::X509, x509::X509NameBuilder,
};
mod client_identity;
mod framing;
mod trust;

/// Creates a fresh synthetic self-signed certificate and its Ed25519 key.
///
/// # Panics
/// Panics if OpenSSL cannot create or encode the synthetic test identity.
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
