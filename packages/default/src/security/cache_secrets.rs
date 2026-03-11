use anyhow::{anyhow, Context, Result};
use base64::{engine::general_purpose, Engine as _};
use rand::RngCore;
use ring::aead::{Aad, LessSafeKey, Nonce, UnboundKey, AES_256_GCM, NONCE_LEN};
use sha2::{Digest, Sha256};

const ENC_PREFIX: &str = "enc:v1:";
const CACHE_ENCRYPTION_KEY_ENV: &str = "CRYSTAL_FORGE_CACHE_ENCRYPTION_KEY";
const FALLBACK_SECRET_KEY_ENV: &str = "CRYSTAL_FORGE_SECRET_KEY";

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

pub fn encrypt_secret(value: &str) -> Result<String> {
    if value.trim().is_empty() {
        return Ok(value.to_string());
    }
    if is_encrypted(value) {
        return Ok(value.to_string());
    }

    let key = open_key()?;
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

pub fn decrypt_secret(value: &str) -> Result<String> {
    if value.trim().is_empty() {
        return Ok(value.to_string());
    }
    if !is_encrypted(value) {
        return Ok(value.to_string());
    }

    let (nonce_bytes, mut in_out) =
        parse_encrypted_payload(value).ok_or_else(|| anyhow!("invalid encrypted secret format"))?;
    let nonce = Nonce::assume_unique_for_key(nonce_bytes);

    let key = open_key()?;
    let plain = key
        .open_in_place(nonce, Aad::empty(), &mut in_out)
        .map_err(|_| anyhow!("failed to decrypt cache secret"))?;

    String::from_utf8(plain.to_vec()).context("decrypted secret is not valid utf-8")
}

pub fn encrypt_optional(value: Option<&str>) -> Result<Option<String>> {
    value.map(encrypt_secret).transpose()
}

pub fn decrypt_optional(value: Option<&str>) -> Result<Option<String>> {
    value.map(decrypt_secret).transpose()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn set_test_key() {
        unsafe {
            std::env::set_var(CACHE_ENCRYPTION_KEY_ENV, "test-cache-encryption-key");
        }
    }

    #[test]
    fn encrypt_decrypt_round_trip() {
        set_test_key();
        let secret = "super-secret-value";
        let encrypted = encrypt_secret(secret).expect("encrypt");
        assert!(is_encrypted(&encrypted));
        let decrypted = decrypt_secret(&encrypted).expect("decrypt");
        assert_eq!(decrypted, secret);
    }

    #[test]
    fn decrypt_plaintext_legacy_value() {
        set_test_key();
        let plaintext = "legacy-plaintext";
        let decrypted = decrypt_secret(plaintext).expect("decrypt legacy");
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn prefix_like_plaintext_still_encrypts() {
        set_test_key();
        let plaintext = "enc:v1:not-base64.not-base64";
        let encrypted = encrypt_secret(plaintext).expect("encrypt");
        assert_ne!(encrypted, plaintext);
        let decrypted = decrypt_secret(&encrypted).expect("decrypt");
        assert_eq!(decrypted, plaintext);
    }
}
