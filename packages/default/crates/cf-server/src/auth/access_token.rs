//! Generates, parses and hashes `cfat_v1_` access-token strings.
//!
//! A token has the canonical form `cfat_v1_<32 lowercase hex id>_<43 base64url
//! characters>`. The id is the nonsecret public token id (a UUID in simple
//! form). The secret is 256 bits from the operating system random generator,
//! encoded as unpadded URL-safe base64.
//!
//! SECURITY: Only [`hash_access_token`] output is persisted. This module does
//! not authenticate a request: it holds no database state and does not check
//! revocation, expiry or the owner. The service layer composes those checks.

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use rand::{RngCore, rngs::OsRng};
use sha2::{Digest, Sha256};
use uuid::Uuid;

/// Fixed prefix that identifies an access token and its format version.
pub const TOKEN_PREFIX: &str = "cfat_v1_";

/// Maximum accepted length of a presented token, in bytes.
///
/// The canonical length is 84. The parser rejects longer input before it does
/// any other work so that a hostile header cannot cause large allocations.
pub const MAX_TOKEN_LEN: usize = 128;

const ID_HEX_LEN: usize = 32;
const SECRET_BYTES: usize = 32;
const SECRET_ENCODED_LEN: usize = 43;
const HINT_HEX_LEN: usize = 4;

/// Holds a plaintext token and never prints it.
///
/// `Debug` and `Display` write a fixed placeholder and the type has no
/// `Serialize` implementation. The only way to read the text is
/// [`TokenSecret::expose`], which callers use for the single create or rotate
/// response.
#[derive(Clone)]
pub struct TokenSecret(String);

impl TokenSecret {
    /// Returns the plaintext token.
    ///
    /// SECURITY: The caller MUST NOT log, audit, store or cache the result.
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Debug for TokenSecret {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("TokenSecret(<redacted>)")
    }
}

impl std::fmt::Display for TokenSecret {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("<redacted>")
    }
}

/// Describes a freshly generated token before it is persisted.
///
/// The plaintext exists only in `token`. `hash` and `display_hint` are the
/// values stored in `api_access_tokens`.
#[derive(Debug, Clone)]
pub struct GeneratedAccessToken {
    /// Public token id, equal to the database primary key.
    pub id: Uuid,
    /// Plaintext token, returned to the owner exactly once.
    pub token: TokenSecret,
    /// SHA-256 of the complete canonical token.
    pub hash: [u8; 32],
    /// Nonsecret hint such as `cfat_v1_8d12…`, safe to show in lists.
    pub display_hint: String,
}

/// Generates a new token for the public id `id`.
///
/// The secret comes from the operating system random generator. It is never
/// derived from the id, a password, a timestamp or client input.
pub fn generate_access_token(id: Uuid) -> GeneratedAccessToken {
    let mut secret = [0u8; SECRET_BYTES];
    OsRng.fill_bytes(&mut secret);
    let token = format!(
        "{TOKEN_PREFIX}{}_{}",
        id.simple(),
        URL_SAFE_NO_PAD.encode(secret)
    );
    let hash = hash_access_token(&token);
    GeneratedAccessToken {
        id,
        token: TokenSecret(token),
        hash,
        display_hint: display_hint(id),
    }
}

/// Returns the masked hint for a token id: the prefix, the first four hex
/// digits of the id and an ellipsis (U+2026).
pub fn display_hint(id: Uuid) -> String {
    let hex = id.simple().to_string();
    format!("{TOKEN_PREFIX}{}\u{2026}", &hex[..HINT_HEX_LEN])
}

/// Returns the SHA-256 of the complete canonical token text.
///
/// A fast hash is correct here because the secret has 256 bits of entropy.
pub fn hash_access_token(token: &str) -> [u8; 32] {
    let digest = Sha256::digest(token.as_bytes());
    let mut out = [0u8; 32];
    out.copy_from_slice(&digest);
    out
}

/// Compares two hashes in constant time.
///
/// Returns `false` for slices of different length.
pub fn hashes_equal(left: &[u8], right: &[u8]) -> bool {
    ring::constant_time::verify_slices_are_equal(left, right).is_ok()
}

/// Parses a presented token and returns its public id.
///
/// The parser is strict and bounded. It accepts only input of at most
/// [`MAX_TOKEN_LEN`] bytes with the exact [`TOKEN_PREFIX`], 32 lowercase hex
/// characters, one `_` and a canonical 43-character base64url secret that
/// decodes to 32 bytes. It rejects whitespace, other characters and
/// non-canonical encodings. It returns `None` for every failure, without a
/// reason, so callers cannot disclose why a token failed.
///
/// SECURITY: A successful parse proves only the shape of the text. The caller
/// MUST still look up the id and compare the hash with [`hashes_equal`].
pub fn parse_access_token(presented: &str) -> Option<Uuid> {
    if presented.len() > MAX_TOKEN_LEN || !presented.is_ascii() {
        return None;
    }
    let rest = presented.strip_prefix(TOKEN_PREFIX)?;
    if rest.len() != ID_HEX_LEN + 1 + SECRET_ENCODED_LEN {
        return None;
    }
    let (id_hex, tail) = rest.split_at(ID_HEX_LEN);
    let secret = tail.strip_prefix('_')?;
    if !id_hex
        .bytes()
        .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return None;
    }
    // `decode` rejects characters outside the URL-safe alphabet, padding and
    // non-zero trailing bits, so the encoding is canonical.
    match URL_SAFE_NO_PAD.decode(secret) {
        Ok(bytes) if bytes.len() == SECRET_BYTES => {}
        _ => return None,
    }
    Uuid::try_parse(id_hex).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> (GeneratedAccessToken, String) {
        let generated = generate_access_token(Uuid::new_v4());
        let text = generated.token.expose().to_owned();
        (generated, text)
    }

    #[test]
    fn generated_token_has_canonical_format_and_parses_back() {
        let (generated, text) = sample();
        assert!(text.starts_with(TOKEN_PREFIX));
        assert_eq!(text.len(), 84);
        let id_part = &text[TOKEN_PREFIX.len()..TOKEN_PREFIX.len() + 32];
        assert_eq!(id_part, generated.id.simple().to_string());
        assert_eq!(parse_access_token(&text), Some(generated.id));
        assert_eq!(generated.hash, hash_access_token(&text));
    }

    #[test]
    fn display_hint_uses_prefix_four_hex_digits_and_ellipsis() {
        let id = Uuid::parse_str("8d12ab34-0000-4000-8000-000000000000").unwrap();
        assert_eq!(display_hint(id), "cfat_v1_8d12\u{2026}");
        assert_eq!(
            generate_access_token(id).display_hint,
            "cfat_v1_8d12\u{2026}"
        );
    }

    #[test]
    fn hash_is_deterministic_and_tokens_are_distinct() {
        let (first, first_text) = sample();
        let (second, second_text) = sample();
        assert_eq!(
            hash_access_token(&first_text),
            hash_access_token(&first_text)
        );
        assert_ne!(first_text, second_text);
        assert_ne!(first.hash, second.hash);
        assert_ne!(first.id, second.id);
        let same_id = Uuid::new_v4();
        assert_ne!(
            generate_access_token(same_id).hash,
            generate_access_token(same_id).hash
        );
    }

    #[test]
    fn parse_rejects_malformed_input() {
        let (_, text) = sample();
        let secret = &text[TOKEN_PREFIX.len() + 33..];
        let id = &text[TOKEN_PREFIX.len()..TOKEN_PREFIX.len() + 32];
        let cases = [
            String::new(),
            "cfat_v1_".to_owned(),
            text.replacen("cfat_v1_", "cfat_v2_", 1),
            text.replacen("cfat_v1_", "CFAT_V1_", 1),
            text.replacen("cfat_v1_", "xcfat_v1_", 1),
            format!("{TOKEN_PREFIX}{}_{secret}", id.to_uppercase()),
            format!("{TOKEN_PREFIX}{}_{secret}", &id[..31]),
            format!("{TOKEN_PREFIX}{id}x{secret}"),
            format!("{TOKEN_PREFIX}{id}_{}", &secret[..42]),
            format!("{TOKEN_PREFIX}{id}_{secret}A"),
            format!("{TOKEN_PREFIX}{id}_{}+", &secret[..42]),
            format!("{TOKEN_PREFIX}{id}_{}=", &secret[..42]),
            format!("{TOKEN_PREFIX}{id}_{} ", &secret[..42]),
            format!("{text} "),
            format!(" {text}"),
            format!("{text}\n"),
            format!("Bearer {text}"),
            format!("{TOKEN_PREFIX}{}_{secret}", "g".repeat(32)),
            format!("{TOKEN_PREFIX}{id}_{}\u{e9}", &secret[..41]),
        ];
        for case in cases {
            assert_eq!(parse_access_token(&case), None, "accepted {case:?}");
        }
    }

    #[test]
    fn parse_rejects_non_canonical_trailing_bits() {
        let (_, text) = sample();
        // The final character carries 4 padding bits that must be zero. Of
        // 'A', 'B' only 'A' is canonical.
        let mut canonical = text[..text.len() - 1].to_owned();
        canonical.push('A');
        assert!(parse_access_token(&canonical).is_some());
        let mut non_canonical = text[..text.len() - 1].to_owned();
        non_canonical.push('B');
        assert_eq!(parse_access_token(&non_canonical), None);
    }

    #[test]
    fn parse_enforces_length_bound_before_other_work() {
        let (_, text) = sample();
        let long = format!("{text}{}", "A".repeat(MAX_TOKEN_LEN));
        assert!(long.len() > MAX_TOKEN_LEN);
        assert_eq!(parse_access_token(&long), None);
        assert_eq!(parse_access_token(&"a".repeat(1_000_000)), None);
    }

    #[test]
    fn constant_time_comparison_matches_equality() {
        let (first, _) = sample();
        let (second, _) = sample();
        assert!(hashes_equal(&first.hash, &first.hash));
        assert!(!hashes_equal(&first.hash, &second.hash));
        assert!(!hashes_equal(&first.hash, &first.hash[..31]));
        assert!(!hashes_equal(&first.hash, &[]));
    }

    #[test]
    fn secret_wrapper_and_generated_token_debug_are_redacted() {
        let (generated, text) = sample();
        let secret_part = &text[TOKEN_PREFIX.len() + 33..];
        let rendered = format!("{} {:?} {:?}", generated.token, generated.token, generated);
        assert!(rendered.contains("<redacted>"));
        assert!(!rendered.contains(secret_part));
        assert!(!rendered.contains(&text));
    }
}
