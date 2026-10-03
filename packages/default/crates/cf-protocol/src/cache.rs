//! Cache-related wire types shared between server and builder.

use serde::{Deserialize, Serialize};

/// Validates a named Nix Ed25519 public signing key.
///
/// Accepts `name:encoded-key` with a nonempty name, no whitespace, and a
/// standard padded Base64 payload that decodes to exactly 32 bytes. Validation
/// checks the wire format, not ownership of the corresponding private key.
/// Manual configuration and discovery must use this same contract.
///
/// # Errors
/// Returns a static format error without including the supplied value when the
/// name, delimiter, encoding, or decoded length is invalid.
///
/// # Examples
/// ```
/// use cf_protocol::cache::validate_nix_public_key;
/// assert!(validate_nix_public_key(
///     "cache-1:AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA="
/// ).is_ok());
/// assert!(validate_nix_public_key("cache-1:YWJj").is_err());
/// ```
pub fn validate_nix_public_key(value: &str) -> Result<(), &'static str> {
    use base64::{Engine as _, engine::general_purpose::STANDARD};
    let invalid = "Invalid Nix public signing key: requires name:base64 with 32 decoded bytes and no whitespace";
    let (name, encoded) = value.split_once(':').ok_or(invalid)?;
    if name.is_empty() || value.chars().any(char::is_whitespace) {
        return Err(invalid);
    }
    let decoded = STANDARD.decode(encoded).map_err(|_| invalid)?;
    if decoded.len() != 32 {
        return Err(invalid);
    }
    Ok(())
}

/// Type of cache destination.
///
/// Used in `BuilderCachePushConfig` delivered from server to builder and in the
/// server-side `CacheConfig` TOML section.
#[derive(Clone, Debug, Deserialize, Serialize, Default, PartialEq, Eq)]
pub enum CacheType {
    /// Nix's S3 store backend.
    S3,
    /// Attic's authenticated cache API.
    Attic,
    /// Niks3's separate write API and Nix read endpoint.
    Niks3,
    /// An HTTP Nix binary cache.
    Http,
    /// A generic Nix store destination.
    #[default]
    Nix,
}

impl std::str::FromStr for CacheType {
    type Err = &'static str;

    /// Parses a known cache type, ignoring ASCII case.
    ///
    /// # Errors
    /// Returns a credential-free error for unknown types; never defaults to Nix.
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.to_ascii_lowercase().as_str() {
            "s3" => Ok(Self::S3),
            "attic" => Ok(Self::Attic),
            "niks3" => Ok(Self::Niks3),
            "http" => Ok(Self::Http),
            "nix" => Ok(Self::Nix),
            _ => Err("unsupported cache type"),
        }
    }
}

/// Supplies read-plane authentication independently of write credentials.
///
/// Credential strings contain PEM contents, not filesystem paths. Transport
/// owners must deliver mTLS credentials only over verified confidential links.
#[derive(Clone, Default, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum CacheReadAuth {
    /// Reads a public cache without client authentication.
    #[default]
    None,
    /// Authenticates cache reads with a client certificate and private key.
    Mtls {
        /// PEM-encoded client certificate chain.
        client_certificate: String,
        /// PEM-encoded client private key; never include in diagnostics.
        client_private_key: String,
        /// Optional PEM trust bundle for server certificate verification.
        #[serde(default)]
        ca_certificate: Option<String>,
    },
}

impl<'de> Deserialize<'de> for CacheReadAuth {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        // SECURITY: Serde's tagged unit variants ignore extra fields even with
        // deny_unknown_fields. A struct variant rejects credentials attached
        // to the public-read mode instead of silently discarding them.
        #[derive(Deserialize)]
        #[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
        enum WireAuth {
            None {},
            Mtls {
                client_certificate: String,
                client_private_key: String,
                #[serde(default)]
                ca_certificate: Option<String>,
            },
        }
        Ok(match WireAuth::deserialize(deserializer)? {
            WireAuth::None {} => Self::None,
            WireAuth::Mtls {
                client_certificate,
                client_private_key,
                ca_certificate,
            } => Self::Mtls {
                client_certificate,
                client_private_key,
                ca_certificate,
            },
        })
    }
}

/// Supplies authentication for the Niks3 write API, never for cache reads.
///
/// Credential strings contain secret or PEM contents, not filesystem paths.
/// Serialization carries credentials and must not be used for diagnostic logs.
#[derive(Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Niks3WriteAuth {
    /// Authenticates with a static API bearer token.
    Token {
        /// Secret bearer token; consumers must pass it through a protected file.
        token: String,
    },
    /// Authenticates with a client certificate and private key.
    Mtls {
        /// PEM-encoded client certificate chain.
        client_certificate: String,
        /// PEM-encoded client private key; never include in diagnostics.
        client_private_key: String,
        /// Optional PEM trust bundle for server certificate verification.
        #[serde(default)]
        ca_certificate: Option<String>,
    },
}

impl std::fmt::Debug for CacheReadAuth {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::None => f.write_str("CacheReadAuth::None"),
            Self::Mtls { .. } => f.write_str("CacheReadAuth::Mtls([REDACTED])"),
        }
    }
}

impl std::fmt::Debug for Niks3WriteAuth {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Token { .. } => f.write_str("Niks3WriteAuth::Token([REDACTED])"),
            Self::Mtls { .. } => f.write_str("Niks3WriteAuth::Mtls([REDACTED])"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nix_public_key_enforces_named_standard_base64_ed25519_shape() {
        use base64::{Engine as _, engine::general_purpose::STANDARD};
        let payload = STANDARD.encode([255; 32]);
        assert!(validate_nix_public_key(&format!("cache-1:{payload}")).is_ok());
        for value in [
            String::new(),
            format!(":{payload}"),
            payload.clone(),
            format!("name:{payload}:extra"),
            format!(" name:{payload}"),
            format!("na\u{2003}me:{payload}"),
            format!("name:{payload}\n"),
            format!("name:{}", payload.replace('/', "_")),
            format!("name:{}", payload.trim_end_matches('=')),
            "name:not-base64!".into(),
            "name:".into(),
        ] {
            assert!(validate_nix_public_key(&value).is_err(), "{value:?}");
        }
        for length in [0, 1, 31, 33, 64] {
            assert!(
                validate_nix_public_key(&format!("name:{}", STANDARD.encode(vec![0; length])))
                    .is_err()
            );
        }
    }

    #[test]
    fn cache_types_parse_known_names_and_reject_unknown_names() {
        for (name, expected) in [
            ("S3", CacheType::S3),
            ("attic", CacheType::Attic),
            ("Niks3", CacheType::Niks3),
            ("HTTP", CacheType::Http),
            ("nix", CacheType::Nix),
        ] {
            assert_eq!(name.parse::<CacheType>().unwrap(), expected);
            let json = serde_json::to_value(&expected).unwrap();
            assert_eq!(serde_json::from_value::<CacheType>(json).unwrap(), expected);
        }
        for unknown in ["", "unknown-secret", "s33", " nix "] {
            let error = unknown.parse::<CacheType>().unwrap_err();
            assert_eq!(error, "unsupported cache type");
            assert!(serde_json::from_value::<CacheType>(serde_json::json!(unknown)).is_err());
        }
        assert_eq!(serde_json::to_value(CacheType::Niks3).unwrap(), "Niks3");
    }

    #[test]
    fn authentication_has_tagged_wire_contract_and_redacted_debug() {
        let read = CacheReadAuth::Mtls {
            client_certificate: "secret-cert".into(),
            client_private_key: "secret-key".into(),
            ca_certificate: Some("secret-ca".into()),
        };
        let writes = [
            Niks3WriteAuth::Token {
                token: "secret-token".into(),
            },
            Niks3WriteAuth::Mtls {
                client_certificate: "secret-cert".into(),
                client_private_key: "secret-key".into(),
                ca_certificate: Some("secret-ca".into()),
            },
        ];
        let value = serde_json::to_value(&read).unwrap();
        assert_eq!(value["kind"], "mtls");
        assert_eq!(
            serde_json::from_value::<CacheReadAuth>(value).unwrap(),
            read
        );
        for write in writes {
            let value = serde_json::to_value(&write).unwrap();
            assert!(value["kind"] == "token" || value["kind"] == "mtls");
            assert_eq!(
                serde_json::from_value::<Niks3WriteAuth>(value).unwrap(),
                write
            );
            for secret in ["secret-cert", "secret-key", "secret-ca", "secret-token"] {
                assert!(!format!("{write:?} {read:?}").contains(secret));
            }
        }
        assert_eq!(
            serde_json::to_value(CacheReadAuth::default()).unwrap(),
            serde_json::json!({"kind":"none"})
        );
        let without_ca: CacheReadAuth = serde_json::from_value(serde_json::json!({
            "kind":"mtls", "client_certificate":"cert", "client_private_key":"key"
        }))
        .unwrap();
        assert!(matches!(
            without_ca,
            CacheReadAuth::Mtls {
                ca_certificate: None,
                ..
            }
        ));
    }

    #[test]
    fn invalid_auth_modes_or_incomplete_credentials_fail_closed() {
        for value in [
            serde_json::json!({"kind":"bearer", "token":"secret"}),
            serde_json::json!({"kind":"mtls", "client_certificate":"cert"}),
            serde_json::json!({"kind":"none", "client_private_key":"key"}),
        ] {
            assert!(serde_json::from_value::<CacheReadAuth>(value.clone()).is_err());
            assert!(serde_json::from_value::<Niks3WriteAuth>(value).is_err());
        }
        assert!(
            serde_json::from_value::<Niks3WriteAuth>(serde_json::json!({"kind":"none"})).is_err()
        );
    }

    #[test]
    fn legacy_payload_defaults_and_destination_identity_roundtrip() {
        use crate::agent::RuntimeCacheConfig;
        use crate::builder::{BuilderCachePushConfig, CompleteJobRequest};

        let push: BuilderCachePushConfig = serde_json::from_str("{}").unwrap();
        assert!(push.cache_destination_id.is_none());
        assert!(push.niks3_server_url.is_none());
        assert!(push.niks3_write_auth.is_none());
        let disabled = BuilderCachePushConfig::disabled();
        assert!(disabled.cache_destination_id.is_none());
        assert!(disabled.niks3_write_auth.is_none());
        let completion: CompleteJobRequest = serde_json::from_str("{}").unwrap();
        assert!(completion.cache_destination_id.is_none());
        assert!(!completion.cache_pushed);
        let read: RuntimeCacheConfig = serde_json::from_value(serde_json::json!({
            "cache_type":"Nix", "cache_url":"https://cache.example",
            "cache_public_key":"legacy:key", "attic_cache_name":null
        }))
        .unwrap();
        assert!(read.cache_public_keys.is_empty());
        assert_eq!(read.read_auth, CacheReadAuth::None);
        assert_eq!(read.cache_public_key.as_deref(), Some("legacy:key"));
        let push: BuilderCachePushConfig = serde_json::from_value(serde_json::json!({
            "cache_type":"Niks3", "cache_destination_id":42,
            "niks3_server_url":"https://write.example",
            "niks3_write_auth":{"kind":"token", "token":"secret-token"}
        }))
        .unwrap();
        assert!(!format!("{push:?}").contains("secret-token"));
        let value = serde_json::to_value(&push).unwrap();
        let roundtrip: BuilderCachePushConfig = serde_json::from_value(value).unwrap();
        assert_eq!(roundtrip.cache_destination_id, Some(42));
        assert_eq!(roundtrip.niks3_write_auth, push.niks3_write_auth);
        let completion: CompleteJobRequest = serde_json::from_value(serde_json::json!({
            "cache_pushed":true, "cache_destination_id":42
        }))
        .unwrap();
        assert_eq!(
            serde_json::to_value(completion).unwrap()["cache_destination_id"],
            42
        );
        let read: RuntimeCacheConfig = serde_json::from_value(serde_json::json!({
            "cache_type":"Niks3", "cache_url":"https://cache.example",
            "cache_public_keys":["one:key", "two:key"],
            "read_auth":{"kind":"mtls", "client_certificate":"cert", "client_private_key":"key"}
        }))
        .unwrap();
        let read: RuntimeCacheConfig =
            serde_json::from_value(serde_json::to_value(read).unwrap()).unwrap();
        assert_eq!(read.cache_public_keys, ["one:key", "two:key"]);
        assert!(matches!(read.read_auth, CacheReadAuth::Mtls { .. }));
    }
}
