//! Exercises Go-compatible raw-line framing and credential-safe failures.

use super::*;

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
