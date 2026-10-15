//! Exercises NSS record pairing, AUX validation, and ordinary root selection.

use super::*;
use base64::{Engine, engine::general_purpose::STANDARD};

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
    // A private API root need not have NSS annotations.
    input.push_str(&cert);
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
