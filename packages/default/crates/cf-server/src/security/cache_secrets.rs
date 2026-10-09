use anyhow::{Context, Result, anyhow};
use base64::{Engine as _, engine::general_purpose};
use rand::RngCore;
use ring::aead::{AES_256_GCM, Aad, LessSafeKey, NONCE_LEN, Nonce, UnboundKey};
use sha2::{Digest, Sha256};

const ENC_PREFIX: &str = "enc:v1:";
const CACHE_ENCRYPTION_KEY_ENV: &str = "CRYSTAL_FORGE_CACHE_ENCRYPTION_KEY";
const FALLBACK_SECRET_KEY_ENV: &str = "CRYSTAL_FORGE_SECRET_KEY";

/// Validates a nonempty certificate-only PEM bundle without making requests.
///
/// Only certificate blocks and ASCII whitespace are accepted. Each block must
/// decode to a valid X.509 certificate. Private keys, other PEM labels, comments,
/// and arbitrary text are rejected because certificate fields remain plaintext.
///
/// # Errors
/// Returns a credential-free error for invalid framing, encoding, or certificates.
pub(crate) fn validate_certificate_bundle(value: &str) -> Result<()> {
    const BEGIN: &str = "-----BEGIN CERTIFICATE-----";
    const END: &str = "-----END CERTIFICATE-----";
    let invalid = || anyhow!("requires a certificate-only PEM bundle");
    let mut remaining = value.trim_matches(|ch: char| ch.is_ascii_whitespace());
    let mut count = 0;
    while !remaining.is_empty() {
        let body = remaining.strip_prefix(BEGIN).ok_or_else(invalid)?;
        let (encoded, rest) = body.split_once(END).ok_or_else(invalid)?;
        let encoded: String = encoded
            .chars()
            .filter(|ch| !ch.is_ascii_whitespace())
            .collect();
        let der = general_purpose::STANDARD
            .decode(encoded)
            .map_err(|_| invalid())?;
        let certificate = reqwest::Certificate::from_der(&der).map_err(|_| invalid())?;
        // SECURITY: PEM readers can ignore other blocks and text. Strict framing
        // above accounts for every input byte. Rustls also parses the complete
        // certificate, rejecting arbitrary DER hidden under a certificate label.
        reqwest::Client::builder()
            .use_rustls_tls()
            .no_proxy()
            .tls_built_in_root_certs(false)
            .add_root_certificate(certificate)
            .build()
            .map_err(|_| invalid())?;
        count += 1;
        remaining = rest.trim_matches(|ch: char| ch.is_ascii_whitespace());
    }
    if count == 0 {
        return Err(invalid());
    }
    Ok(())
}

// Frozen self-signed Ed25519 certificate. Tests parse structure, not validity
// dates or trust. No private key is retained in this fixture.
#[cfg(test)]
pub(crate) const TEST_CERTIFICATE: &str = "-----BEGIN CERTIFICATE-----\n\
MIIBYzCCARWgAwIBAgIUUiT5rFbIc6C8wVUsrDv3mmB3PDkwBQYDK2VwMCYxJDAi\n\
BgNVBAMMG3Rhc2s0NzAtY2VydGlmaWNhdGUtZml4dHVyZTAgFw0yNjEwMDMwMDQ1\n\
MTJaGA8yMTI2MDkwOTAwNDUxMlowJjEkMCIGA1UEAwwbdGFzazQ3MC1jZXJ0aWZp\n\
Y2F0ZS1maXh0dXJlMCowBQYDK2VwAyEArmgxjwzB+VI7yxPnKKkVj9uulx0wtyrf\n\
HC/p+1wXxpOjUzBRMB0GA1UdDgQWBBS5G+8DC/fImjFHjQtXKwSe/LTS7TAfBgNV\n\
HSMEGDAWgBS5G+8DC/fImjFHjQtXKwSe/LTS7TAPBgNVHRMBAf8EBTADAQH/MAUG\n\
AytlcANBANv0+9Ic9w11sJMah7/3hykwUUhs+iQqGZWTnWwQumZ7bkfKPl4Illm2\n\
Zh/vQ6oHa2rNyo8ob+V7jS6Zzq1/6Qk=\n\
-----END CERTIFICATE-----\n";

fn load_key_material() -> Result<[u8; 32]> {
    let raw = std::env::var(CACHE_ENCRYPTION_KEY_ENV)
        .ok()
        .or_else(|| std::env::var(FALLBACK_SECRET_KEY_ENV).ok())
        .ok_or_else(|| {
            anyhow!(
                "missing cache encryption key; set {} (or {})",
                CACHE_ENCRYPTION_KEY_ENV,
                FALLBACK_SECRET_KEY_ENV
            )
        })?;

    let mut hasher = Sha256::new();
    hasher.update(raw.as_bytes());
    let digest = hasher.finalize();

    let mut key = [0u8; 32];
    key.copy_from_slice(&digest);
    Ok(key)
}

fn open_key() -> Result<LessSafeKey> {
    let key_bytes = load_key_material()?;
    let unbound = UnboundKey::new(&AES_256_GCM, &key_bytes)
        .context("failed to initialize AES-256-GCM key")?;
    Ok(LessSafeKey::new(unbound))
}

/// Identifies a syntactically valid version-1 encrypted envelope.
///
/// This check does not authenticate the ciphertext; decryption does.
pub fn is_encrypted(value: &str) -> bool {
    parse_encrypted_payload(value).is_some()
}

fn parse_encrypted_payload(value: &str) -> Option<([u8; NONCE_LEN], Vec<u8>)> {
    if !value.starts_with(ENC_PREFIX) {
        return None;
    }

    let encoded = value.trim_start_matches(ENC_PREFIX);
    let mut parts = encoded.splitn(2, '.');
    let nonce_b64 = parts.next()?;
    let ciphertext_b64 = parts.next()?;

    let nonce_vec = general_purpose::STANDARD.decode(nonce_b64).ok()?;
    if nonce_vec.len() != NONCE_LEN {
        return None;
    }
    let mut nonce_bytes = [0u8; NONCE_LEN];
    nonce_bytes.copy_from_slice(&nonce_vec);

    let ciphertext = general_purpose::STANDARD.decode(ciphertext_b64).ok()?;
    Some((nonce_bytes, ciphertext))
}

/// Encrypts a cache token or private key with the configured AES-256-GCM key.
///
/// Empty values and existing encrypted envelopes remain unchanged for legacy
/// compatibility. Each new encryption uses a random 96-bit nonce.
///
/// # Errors
/// Returns an error if key loading or authenticated encryption fails.
pub fn encrypt_secret(value: &str) -> Result<String> {
    if value.trim().is_empty() {
        return Ok(value.to_string());
    }
    if is_encrypted(value) {
        return Ok(value.to_string());
    }

    let key = open_key()?;
    encrypt_with_key(value, &key)
}

fn encrypt_with_key(value: &str, key: &LessSafeKey) -> Result<String> {
    let mut nonce_bytes = [0u8; NONCE_LEN];
    rand::rngs::OsRng.fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::assume_unique_for_key(nonce_bytes);

    let mut in_out = value.as_bytes().to_vec();
    key.seal_in_place_append_tag(nonce, Aad::empty(), &mut in_out)
        .context("failed to encrypt cache secret")?;

    Ok(format!(
        "{}{}.{}",
        ENC_PREFIX,
        general_purpose::STANDARD.encode(nonce_bytes),
        general_purpose::STANDARD.encode(in_out)
    ))
}

/// Decrypts an encrypted cache credential, accepting legacy plaintext values.
///
/// # Errors
/// Returns an error if key loading, authentication, or UTF-8 decoding fails.
pub fn decrypt_secret(value: &str) -> Result<String> {
    if value.trim().is_empty() {
        return Ok(value.to_string());
    }
    if !is_encrypted(value) {
        return Ok(value.to_string());
    }

    let key = open_key()?;
    decrypt_with_key(value, &key)
}

fn decrypt_with_key(value: &str, key: &LessSafeKey) -> Result<String> {
    let (nonce_bytes, mut in_out) =
        parse_encrypted_payload(value).ok_or_else(|| anyhow!("invalid encrypted secret format"))?;
    let nonce = Nonce::assume_unique_for_key(nonce_bytes);

    let plain = key
        .open_in_place(nonce, Aad::empty(), &mut in_out)
        .map_err(|_| anyhow!("failed to decrypt cache secret"))?;

    String::from_utf8(plain.to_vec()).context("decrypted secret is not valid utf-8")
}

/// Encrypts a present credential, preserving an absent value.
///
/// # Errors
/// Returns the encryption errors from [`encrypt_secret`].
pub fn encrypt_optional(value: Option<&str>) -> Result<Option<String>> {
    value.map(encrypt_secret).transpose()
}

/// Encrypts a plaintext Basic password without treating spaces as an empty value.
///
/// Basic passwords retain exact bytes, including surrounding or all-space
/// values. Unlike the legacy helper, this always encrypts a present plaintext
/// value, even when that value resembles an envelope. Retained stored envelopes
/// must be preserved by callers instead of passed as plaintext.
///
/// # Errors
/// Returns a static error for an empty password or key/encryption failure.
pub(crate) fn encrypt_basic_password(value: Option<&str>) -> Result<Option<String>> {
    value
        .map(|value| {
            if value.is_empty() {
                return Err(anyhow!("Basic password cannot be empty"));
            }
            encrypt_with_key(value, &open_key()?)
        })
        .transpose()
}

/// Decrypts a present credential, preserving an absent value.
///
/// # Errors
/// Returns the decryption errors from [`decrypt_secret`].
pub fn decrypt_optional(value: Option<&str>) -> Result<Option<String>> {
    value.map(decrypt_secret).transpose()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn certificate_bundles_require_only_valid_x509_blocks() {
        validate_certificate_bundle(TEST_CERTIFICATE).unwrap();
        validate_certificate_bundle(&format!(" \n{TEST_CERTIFICATE}\t\n{TEST_CERTIFICATE} \n"))
            .unwrap();
        for bad in [
            String::new(),
            "arbitrary data".into(),
            format!("prefix\n{TEST_CERTIFICATE}"),
            format!("{TEST_CERTIFICATE}\nsuffix"),
            format!(
                "{TEST_CERTIFICATE}\n-----BEGIN PUBLIC KEY-----\nAQID\n-----END PUBLIC KEY-----"
            ),
            "-----BEGIN CERTIFICATE-----\nAQID\n-----END CERTIFICATE-----".into(),
            TEST_CERTIFICATE.replace("MIIBYz", "!IIBYz"),
            TEST_CERTIFICATE.replace("-----END CERTIFICATE-----", ""),
        ] {
            assert!(validate_certificate_bundle(&bad).is_err());
        }
        for label in [
            "PRIVATE KEY",
            "RSA PRIVATE KEY",
            "EC PRIVATE KEY",
            "ENCRYPTED PRIVATE KEY",
        ] {
            let key = format!("-----BEGIN {label}-----\nAQID\n-----END {label}-----");
            for bad in [
                key.clone(),
                format!("{key}\n{TEST_CERTIFICATE}"),
                format!("{TEST_CERTIFICATE}\n{key}"),
            ] {
                assert!(validate_certificate_bundle(&bad).is_err());
            }
        }
        let body = TEST_CERTIFICATE
            .strip_prefix("-----BEGIN CERTIFICATE-----")
            .unwrap()
            .split_once("-----END CERTIFICATE-----")
            .unwrap()
            .0;
        let encoded: String = body
            .chars()
            .filter(|ch| !ch.is_ascii_whitespace())
            .collect();
        let mut der = general_purpose::STANDARD.decode(encoded).unwrap();
        der.extend_from_slice(b"hidden private material");
        let trailing_der = format!(
            "-----BEGIN CERTIFICATE-----\n{}\n-----END CERTIFICATE-----",
            general_purpose::STANDARD.encode(der)
        );
        assert!(validate_certificate_bundle(&trailing_der).is_err());
    }

    fn test_key(bytes: &[u8; 32]) -> LessSafeKey {
        LessSafeKey::new(UnboundKey::new(&AES_256_GCM, bytes).expect("test AES key"))
    }

    #[test]
    fn certificate_bundle_regressions_reject_interleaved_comments_and_noncert_blocks() {
        let multi = format!("\n{TEST_CERTIFICATE}\t\n{TEST_CERTIFICATE}\n");
        assert!(validate_certificate_bundle(&multi).is_ok());
        for noncert in [
            "# certificate comment",
            "arbitrary trailing text",
            "-----BEGIN PRIVATE KEY-----\nAQID\n-----END PRIVATE KEY-----",
            "-----BEGIN CERTIFICATE-----\nnot-base64!\n-----END CERTIFICATE-----",
        ] {
            for candidate in [
                format!("{TEST_CERTIFICATE}\n{noncert}\n{TEST_CERTIFICATE}"),
                format!("{noncert}\n{TEST_CERTIFICATE}"),
                format!("{TEST_CERTIFICATE}\n{noncert}"),
            ] {
                let error = validate_certificate_bundle(&candidate)
                    .unwrap_err()
                    .to_string();
                assert_eq!(error, "requires a certificate-only PEM bundle");
                assert!(!error.contains("BEGIN"));
            }
        }
    }

    #[test]
    fn encrypt_decrypt_round_trip() {
        let key = test_key(&[7; 32]);
        let secret = "super-secret-value";
        let encrypted = encrypt_with_key(secret, &key).expect("encrypt");
        assert!(is_encrypted(&encrypted));
        let decrypted = decrypt_with_key(&encrypted, &key).expect("decrypt");
        assert_eq!(decrypted, secret);
    }

    #[test]
    fn decrypt_plaintext_legacy_value() {
        let plaintext = "legacy-plaintext";
        let decrypted = decrypt_secret(plaintext).expect("decrypt legacy");
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn basic_password_envelope_preserves_spaces_and_literal_prefixes() {
        let key = test_key(&[7; 32]);
        for plaintext in ["  ", " leading and trailing ", "enc:v1:literal-password"] {
            let ciphertext = encrypt_with_key(plaintext, &key).unwrap();
            assert!(is_encrypted(&ciphertext));
            assert!(decrypt_with_key(&ciphertext, &key).unwrap() == plaintext);
            assert!(!ciphertext.contains(plaintext));
        }
    }

    #[test]
    fn prefix_like_plaintext_still_encrypts() {
        let key = test_key(&[7; 32]);
        let plaintext = "enc:v1:not-base64.not-base64";
        let encrypted = encrypt_with_key(plaintext, &key).expect("encrypt");
        assert_ne!(encrypted, plaintext);
        let decrypted = decrypt_with_key(&encrypted, &key).expect("decrypt");
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn niks3_tokens_and_private_keys_are_authenticated_and_randomized() {
        let key = test_key(&[7; 32]);
        let wrong_key = test_key(&[8; 32]);
        for plaintext in [
            "niks3-write-token",
            "-----BEGIN PRIVATE KEY-----\nwrite\n",
            "-----BEGIN PRIVATE KEY-----\nread\n",
        ] {
            let first = encrypt_with_key(plaintext, &key).unwrap();
            let second = encrypt_with_key(plaintext, &key).unwrap();
            assert_ne!(first, second);
            assert!(!first.contains(plaintext));
            assert_eq!(decrypt_with_key(&first, &key).unwrap(), plaintext);
            assert!(decrypt_with_key(&first, &wrong_key).is_err());
            let (nonce, mut ciphertext) = parse_encrypted_payload(&first).unwrap();
            ciphertext[0] ^= 1;
            let tampered = format!(
                "{ENC_PREFIX}{}.{}",
                general_purpose::STANDARD.encode(nonce),
                general_purpose::STANDARD.encode(ciphertext)
            );
            assert!(decrypt_with_key(&tampered, &key).is_err());
            assert_eq!(encrypt_secret(&first).unwrap(), first);
        }
        assert_eq!(encrypt_optional(None).unwrap(), None);
        assert_eq!(decrypt_optional(None).unwrap(), None);
    }
}
