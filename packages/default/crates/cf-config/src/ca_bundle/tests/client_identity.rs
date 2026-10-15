//! Exercises client chain/key matching independently of CA trust selection.

use super::*;

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
    let ec = EcKey::generate(&EcGroup::from_curve_name(Nid::X9_62_PRIME256V1).unwrap()).unwrap();
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
