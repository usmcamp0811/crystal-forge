use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;

/// Cache destination to environment assignment
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct CacheDestinationEnvironment {
    pub cache_destination_id: i32,
    pub environment_id: uuid::Uuid,
    pub created_at: DateTime<Utc>,
}

/// Stores a cache destination with separate Niks3 read and write credentials.
///
/// Query helpers decrypt tokens and private keys and compute configured flags.
/// Serialization omits all tokens, S3 access IDs/secrets, and private keys.
/// Debug omits all credentials. Attic/S3 configured flags describe only the active
/// cache type and must be refreshed from decrypted fields before redaction.
/// Niks3 requires HTTPS URLs, nonempty signing keys, and complete selected auth.
/// Client certificates and CA bundles contain only X.509 certificate PEM blocks.
/// Custom CA bundles are supported only for mTLS modes, not token/public modes.
/// Niks3 URLs follow the prepared read helper's credential-query policy. Tokens
/// and TLS credentials must use dedicated fields, not URL query parameters.
#[derive(Clone, Default, Serialize, Deserialize, FromRow)]
pub struct CacheDestination {
    pub id: i32,
    pub name: String,
    /// Destination implementation: S3, Attic, Http, Nix, or Niks3.
    pub cache_type: String,

    // Common fields
    /// Stores the raw cache URL internally, including legacy URI authentication.
    /// Cache-management responses redact userinfo and credential queries. Save
    /// and Test retain same-type sanitized round trips without revealing auth.
    pub push_to: Option<String>,
    pub enabled: bool,
    pub signing_key_path: Option<String>,
    pub compression: Option<String>,

    // S3-specific
    pub s3_region: Option<String>,
    pub s3_profile: Option<String>,
    /// Stores the decrypted internal S3 access ID; omitted from serialization.
    #[serde(skip_serializing)]
    pub s3_access_key_id: Option<String>,
    /// Stores the decrypted internal S3 secret; omitted from serialization.
    #[serde(skip_serializing)]
    pub s3_secret_access_key: Option<String>,
    /// Stores the optional decrypted S3 session token; omitted from serialization.
    #[serde(skip_serializing)]
    pub s3_session_token: Option<String>,
    /// Stores the raw S3 endpoint; API responses redact legacy URI credentials.
    /// Credential queries are never replayed by the read-only Test operation.
    pub s3_endpoint_url: Option<String>,

    // Attic-specific
    /// Stores the decrypted internal Attic token; omitted from serialization.
    #[serde(skip_serializing)]
    pub attic_token: Option<String>,
    /// Indicates a nonempty decrypted token for the active Attic type.
    #[serde(default)]
    #[sqlx(skip)]
    pub attic_token_configured: bool,
    /// Indicates a complete decrypted access ID and secret for the active S3 type.
    #[serde(default)]
    #[sqlx(skip)]
    pub s3_credentials_configured: bool,
    /// Indicates a nonempty decrypted session token for the active S3 type.
    #[serde(default)]
    #[sqlx(skip)]
    pub s3_session_token_configured: bool,
    /// Indicates stored URL userinfo for the active Http or Nix type.
    /// The username and password remain server-only.
    #[serde(default)]
    #[sqlx(skip)]
    pub http_basic_auth_configured: bool,
    /// Indicates recognized credential queries in either legacy URL field.
    /// Such queries are retained in storage but must never be replayed by Test.
    #[serde(default)]
    #[sqlx(skip)]
    pub legacy_query_credentials_configured: bool,
    pub attic_cache_name: Option<String>,
    pub attic_public_key: Option<String>,
    pub attic_ignore_upstream_cache_filter: Option<bool>,
    pub attic_jobs: Option<i32>,

    /// HTTPS Niks3 write API URL, independent of the read URL in `push_to`.
    pub niks3_server_url: Option<String>,
    /// Trusted Nix signing keys; Niks3 requires at least one nonempty key.
    #[serde(default)]
    pub niks3_public_keys: Vec<String>,
    /// Write authentication mode: `token` or `mtls`.
    pub niks3_write_auth_mode: Option<String>,
    /// Write token, decrypted internally and omitted from API serialization.
    #[serde(skip_serializing)]
    pub niks3_auth_token: Option<String>,
    /// PEM client certificate for mTLS writes.
    pub niks3_write_client_cert: Option<String>,
    /// PEM private key for mTLS writes; omitted from API serialization.
    #[serde(skip_serializing)]
    pub niks3_write_client_key: Option<String>,
    /// Optional PEM CA certificate for the write server.
    pub niks3_write_ca_cert: Option<String>,
    /// Read authentication mode: `none` or `mtls`.
    pub niks3_read_auth_mode: Option<String>,
    /// PEM client certificate for mTLS reads.
    pub niks3_read_client_cert: Option<String>,
    /// PEM private key for mTLS reads; omitted from API serialization.
    #[serde(skip_serializing)]
    pub niks3_read_client_key: Option<String>,
    /// Optional PEM CA certificate for the read server.
    pub niks3_read_ca_cert: Option<String>,
    /// Indicates a nonempty write token on the active Niks3 type.
    #[sqlx(skip)]
    #[serde(default)]
    pub niks3_write_token_configured: bool,
    /// Indicates a complete write certificate/key pair on the active Niks3 type.
    #[sqlx(skip)]
    #[serde(default)]
    pub niks3_write_mtls_configured: bool,
    /// Indicates a complete read certificate/key pair on the active Niks3 type.
    #[sqlx(skip)]
    #[serde(default)]
    pub niks3_read_mtls_configured: bool,

    // Performance tuning
    pub parallel_uploads: Option<i32>,
    pub max_retries: Option<i32>,
    pub retry_delay_seconds: Option<i64>,
    pub push_timeout_seconds: Option<i64>,

    // Push behavior
    pub force_repush: Option<bool>,
    pub require_sigs: Option<bool>,

    // Timestamps
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub last_used_at: Option<DateTime<Utc>>,
}

/// Creates a destination from plaintext credentials and type-specific settings.
///
/// Call [`Self::validate`] before persistence. Niks3 read and write auth modes
/// are explicit; credentials from an unselected mode are rejected.
#[derive(Clone, Default, Serialize, Deserialize)]
pub struct CreateCacheDestination {
    pub name: String,
    pub cache_type: String,
    pub push_to: Option<String>,
    pub enabled: Option<bool>,
    pub signing_key_path: Option<String>,
    pub compression: Option<String>,
    pub s3_region: Option<String>,
    pub s3_profile: Option<String>,
    pub s3_access_key_id: Option<String>,
    pub s3_secret_access_key: Option<String>,
    pub s3_session_token: Option<String>,
    pub s3_endpoint_url: Option<String>,
    pub attic_token: Option<String>,
    pub attic_cache_name: Option<String>,
    pub attic_public_key: Option<String>,
    pub attic_ignore_upstream_cache_filter: Option<bool>,
    pub attic_jobs: Option<i32>,
    /// HTTPS write API URL, separate from the read URL in `push_to`.
    pub niks3_server_url: Option<String>,
    /// Trusted Nix signing keys; at least one nonempty key is required.
    #[serde(default)]
    pub niks3_public_keys: Vec<String>,
    /// Required write mode for Niks3: `token` or `mtls`.
    pub niks3_write_auth_mode: Option<String>,
    /// Write token; persisted with AES-256-GCM encryption.
    pub niks3_auth_token: Option<String>,
    /// PEM client certificate for mTLS writes.
    pub niks3_write_client_cert: Option<String>,
    /// PEM private key for mTLS writes; persisted encrypted.
    pub niks3_write_client_key: Option<String>,
    /// Optional PEM write server CA certificate.
    pub niks3_write_ca_cert: Option<String>,
    /// Required read mode for Niks3: `none` or `mtls`.
    pub niks3_read_auth_mode: Option<String>,
    /// PEM client certificate for mTLS reads.
    pub niks3_read_client_cert: Option<String>,
    /// PEM private key for mTLS reads; persisted encrypted.
    pub niks3_read_client_key: Option<String>,
    /// Optional PEM read server CA certificate.
    pub niks3_read_ca_cert: Option<String>,
    pub parallel_uploads: Option<i32>,
    pub max_retries: Option<i32>,
    pub retry_delay_seconds: Option<i64>,
    pub push_timeout_seconds: Option<i64>,
    pub force_repush: Option<bool>,
    pub require_sigs: Option<bool>,
    // Environment assignments (empty = global cache)
    pub environment_ids: Option<Vec<uuid::Uuid>>,
}

/// Specifies partial destination updates and explicit Niks3 secret clears.
///
/// Omitted fields preserve existing values. Authentication mode changes clear
/// the previous credential set before applying replacements. The merged state
/// must validate before persistence. A clear cannot remove a required credential
/// unless the same update selects a mode that no longer requires that credential.
/// Same-type sanitized URL round trips retain server-only URI credentials.
/// Other explicit URLs cannot borrow URI auth. Cache type changes strip URI
/// auth from inherited URLs; explicit replacement URLs use their own auth.
#[derive(Clone, Default, Serialize, Deserialize)]
pub struct UpdateCacheDestination {
    pub name: Option<String>,
    pub cache_type: Option<String>,
    pub push_to: Option<String>,
    pub enabled: Option<bool>,
    pub signing_key_path: Option<String>,
    pub compression: Option<String>,
    pub s3_region: Option<String>,
    pub s3_profile: Option<String>,
    pub s3_access_key_id: Option<String>,
    pub s3_secret_access_key: Option<String>,
    pub s3_session_token: Option<String>,
    pub s3_endpoint_url: Option<String>,
    pub attic_token: Option<String>,
    pub attic_cache_name: Option<String>,
    pub attic_public_key: Option<String>,
    pub attic_ignore_upstream_cache_filter: Option<bool>,
    pub attic_jobs: Option<i32>,
    /// Replaces the HTTPS write API URL when supplied.
    pub niks3_server_url: Option<String>,
    /// Replaces signing keys when nonempty; omission preserves existing keys.
    #[serde(default)]
    pub niks3_public_keys: Vec<String>,
    /// Replaces write mode and clears credentials from the previous mode.
    pub niks3_write_auth_mode: Option<String>,
    /// Replaces the write token when supplied.
    pub niks3_auth_token: Option<String>,
    /// Replaces the write client certificate when supplied.
    pub niks3_write_client_cert: Option<String>,
    /// Replaces the write private key when supplied.
    pub niks3_write_client_key: Option<String>,
    /// Replaces the write CA certificate when supplied.
    pub niks3_write_ca_cert: Option<String>,
    /// Replaces read mode; `none` clears all read mTLS material.
    pub niks3_read_auth_mode: Option<String>,
    /// Replaces the read client certificate when supplied.
    pub niks3_read_client_cert: Option<String>,
    /// Replaces the read private key when supplied.
    pub niks3_read_client_key: Option<String>,
    /// Replaces the read CA certificate when supplied.
    pub niks3_read_ca_cert: Option<String>,
    /// Clears the write token; a simultaneous replacement is rejected.
    #[serde(default)]
    pub clear_niks3_auth_token: bool,
    /// Clears the write private key; a simultaneous replacement is rejected.
    #[serde(default)]
    pub clear_niks3_write_client_key: bool,
    /// Clears the read private key; a simultaneous replacement is rejected.
    #[serde(default)]
    pub clear_niks3_read_client_key: bool,
    /// Clears the optional write CA in the current mTLS mode or a mode transition.
    /// A simultaneous replacement is rejected.
    #[serde(default)]
    pub clear_niks3_write_ca_cert: bool,
    /// Clears the optional read CA in the current mTLS mode or a mode transition.
    /// A simultaneous replacement is rejected.
    #[serde(default)]
    pub clear_niks3_read_ca_cert: bool,
    pub parallel_uploads: Option<i32>,
    pub max_retries: Option<i32>,
    pub retry_delay_seconds: Option<i64>,
    pub push_timeout_seconds: Option<i64>,
    pub force_repush: Option<bool>,
    pub require_sigs: Option<bool>,
    // Environment assignments (None = don't change, Some(vec) = update assignments)
    pub environment_ids: Option<Vec<uuid::Uuid>>,
}

impl CreateCacheDestination {
    /// Validates required fields and authentication for the selected cache type.
    ///
    /// Niks3 requires at least one named Ed25519 public signing key accepted by
    /// [`cf_protocol::cache::validate_nix_public_key`], in every auth mode.
    /// Validation does not contact either cache endpoint.
    ///
    /// # Errors
    /// Returns an error for unknown types, malformed keys, or incomplete
    /// type-specific settings.
    pub fn validate(&self) -> Result<(), String> {
        // Validate cache type
        match self.cache_type.as_str() {
            "S3" | "Attic" | "Http" | "Nix" | "Niks3" => {}
            _ => {
                return Err(
                    "Invalid cache_type. Must be one of: S3, Attic, Http, Nix, Niks3".into(),
                );
            }
        }

        // Validate name is not empty
        if self.name.trim().is_empty() {
            return Err("Cache destination name cannot be empty".to_string());
        }

        // SECURITY: Check every supplied certificate field, including fields on
        // other cache types, before any plaintext certificate can be persisted.
        validate_certificate_fields(&[
            (
                "niks3_write_client_cert",
                self.niks3_write_client_cert.as_deref(),
            ),
            ("niks3_write_ca_cert", self.niks3_write_ca_cert.as_deref()),
            (
                "niks3_read_client_cert",
                self.niks3_read_client_cert.as_deref(),
            ),
            ("niks3_read_ca_cert", self.niks3_read_ca_cert.as_deref()),
        ])?;

        // Type-specific validation
        match self.cache_type.as_str() {
            "Niks3" => {
                validate_https(self.niks3_server_url.as_deref(), "niks3_server_url")?;
                validate_https(self.push_to.as_deref(), "push_to")?;
                if self.niks3_public_keys.is_empty() {
                    return Err("niks3_public_keys requires nonempty signing keys".into());
                }
                for key in &self.niks3_public_keys {
                    cf_protocol::cache::validate_nix_public_key(key).map_err(str::to_string)?;
                }
                validate_write_auth(
                    self.niks3_write_auth_mode.as_deref(),
                    self.niks3_auth_token.as_deref(),
                    self.niks3_write_client_cert.as_deref(),
                    self.niks3_write_client_key.as_deref(),
                    self.niks3_write_ca_cert.as_deref(),
                )?;
                validate_read_auth(
                    self.niks3_read_auth_mode.as_deref(),
                    self.niks3_read_client_cert.as_deref(),
                    self.niks3_read_client_key.as_deref(),
                    self.niks3_read_ca_cert.as_deref(),
                )?;
            }
            "Attic" => {
                if self.push_to.is_none()
                    || self
                        .push_to
                        .as_ref()
                        .map(|s| s.trim().is_empty())
                        .unwrap_or(true)
                {
                    return Err("push_to URL is required for Attic cache type".to_string());
                }

                if self.attic_cache_name.is_none()
                    || self
                        .attic_cache_name
                        .as_ref()
                        .map(|s| s.trim().is_empty())
                        .unwrap_or(true)
                {
                    return Err("attic_cache_name is required for Attic cache type".to_string());
                }

                if self.attic_public_key.is_none()
                    || self
                        .attic_public_key
                        .as_ref()
                        .map(|s| s.trim().is_empty())
                        .unwrap_or(true)
                {
                    return Err("attic_public_key is required for Attic cache type".to_string());
                }

                if self.attic_token.is_none()
                    || self
                        .attic_token
                        .as_ref()
                        .map(|s| s.trim().is_empty())
                        .unwrap_or(true)
                {
                    return Err("attic_token is required for Attic cache type".to_string());
                }
            }
            "S3" | "Http" | "Nix" => {
                if self.push_to.is_none()
                    || self
                        .push_to
                        .as_ref()
                        .map(|s| s.trim().is_empty())
                        .unwrap_or(true)
                {
                    return Err(format!(
                        "push_to URL is required for {} cache type",
                        self.cache_type
                    ));
                }

                if self.cache_type == "S3" {
                    if self.s3_region.is_none()
                        || self
                            .s3_region
                            .as_ref()
                            .map(|s| s.trim().is_empty())
                            .unwrap_or(true)
                    {
                        return Err("s3_region is required for S3 cache type".to_string());
                    }

                    if self.s3_access_key_id.is_none()
                        || self
                            .s3_access_key_id
                            .as_ref()
                            .map(|s| s.trim().is_empty())
                            .unwrap_or(true)
                    {
                        return Err("s3_access_key_id is required for S3 cache type".to_string());
                    }

                    if self.s3_secret_access_key.is_none()
                        || self
                            .s3_secret_access_key
                            .as_ref()
                            .map(|s| s.trim().is_empty())
                            .unwrap_or(true)
                    {
                        return Err(
                            "s3_secret_access_key is required for S3 cache type".to_string()
                        );
                    }

                    if self.s3_endpoint_url.is_none()
                        || self
                            .s3_endpoint_url
                            .as_ref()
                            .map(|s| s.trim().is_empty())
                            .unwrap_or(true)
                    {
                        return Err("s3_endpoint_url is required for S3 cache type".to_string());
                    }
                }
            }
            _ => {}
        }

        Ok(())
    }
}

// SECURITY: Debug never formats credential-bearing fields, including DTOs.
macro_rules! redacted_debug {
    ($($ty:ty),+) => {$(
        impl std::fmt::Debug for $ty {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.debug_struct(stringify!($ty))
                    .field("name", &self.name)
                    .field("cache_type", &self.cache_type)
                    .finish_non_exhaustive()
            }
        }
    )+};
}
redacted_debug!(
    CacheDestination,
    CreateCacheDestination,
    UpdateCacheDestination
);

fn nonempty(value: Option<&str>) -> bool {
    value.is_some_and(|value| !value.trim().is_empty())
}

fn validate_https(value: Option<&str>, field: &str) -> Result<(), String> {
    let invalid = || {
        format!(
            "{field} requires HTTPS without embedded credentials, fragments, or credential query parameters"
        )
    };
    let value = value.ok_or_else(invalid)?;
    // SECURITY: Public-read preparation performs only URL/key validation and
    // allocates no credential files. Reuse its decoded, case-insensitive query
    // policy so persisted Niks3 URLs cannot bypass managed credential fields.
    cf_config::cache_credentials::PreparedCacheRead::new(
        value,
        &[],
        &cf_protocol::cache::CacheReadAuth::None,
    )
    .map_err(|_| invalid())?;
    let url = url::Url::parse(value).map_err(|_| invalid())?;
    if url.scheme() != "https" {
        return Err(invalid());
    }
    Ok(())
}

/// Identifies a decoded query name prohibited by the prepared cache URL policy.
///
/// Uses public-read preparation on a fixed URL with one encoded query pair.
/// Preparation has no filesystem or network side effects in public-read mode.
/// The caller must pass decoded names from [`url::Url::query_pairs`].
pub(crate) fn niks3_query_parameter_is_sensitive(name: &str) -> bool {
    let query = url::form_urlencoded::Serializer::new(String::new())
        .append_pair(name, "")
        .finish();
    cf_config::cache_credentials::PreparedCacheRead::new(
        &format!("https://cache.invalid/?{query}"),
        &[],
        &cf_protocol::cache::CacheReadAuth::None,
    )
    .is_err()
}

/// Identifies decoded credential-query names, including legacy AWS signatures.
///
/// Matching is case-insensitive. Callers must percent-decode query names once
/// with [`url::Url::query_pairs`] or [`url::form_urlencoded::parse`]. The existing
/// Niks3 managed-credential policy also remains in force.
pub(crate) fn cache_url_query_parameter_is_sensitive(name: &str) -> bool {
    let normalized: String = name
        .chars()
        .filter(|ch| !matches!(*ch, '-' | '_'))
        .flat_map(char::to_lowercase)
        .collect();
    matches!(
        normalized.as_str(),
        "xamzcredential"
            | "xamzsecuritytoken"
            | "xamzsignature"
            | "awsaccesskeyid"
            | "signature"
            | "securitytoken"
            | "accesskeyid"
            | "secretaccesskey"
            | "sessiontoken"
    ) || niks3_query_parameter_is_sensitive(name)
}

/// Detects credential queries without exposing their names or values.
///
/// Also examines query text on malformed legacy URLs. Redaction and refusal
/// must not depend on a malformed credential-bearing URL being parseable.
pub(crate) fn cache_url_has_query_credentials(raw: &str) -> bool {
    if let Ok(url) = url::Url::parse(raw) {
        return url
            .query_pairs()
            .any(|(name, _)| cache_url_query_parameter_is_sensitive(&name));
    }
    let query = raw
        .split('#')
        .next()
        .unwrap_or("")
        .split_once('?')
        .map(|(_, query)| query);
    query.is_some_and(|query| {
        url::form_urlencoded::parse(query.as_bytes())
            .any(|(name, _)| cache_url_query_parameter_is_sensitive(&name))
    })
}

/// Removes URL userinfo, credential queries, and fragments for API responses.
///
/// Invalid or opaque URLs return `[REDACTED]`; unparsed input is never echoed.
/// Safe query pairs remain in order. The returned URL is presentation metadata,
/// not a replacement for the stored authenticated URL.
pub(crate) fn sanitize_cache_url_credentials(raw: &str) -> String {
    if raw.bytes().any(|byte| byte.is_ascii_control()) {
        return "[REDACTED]".into();
    }
    let Ok(mut parsed) = url::Url::parse(raw) else {
        return "[REDACTED]".into();
    };
    if parsed.cannot_be_a_base() {
        return "[REDACTED]".into();
    }
    let sensitive_query = cache_url_has_query_credentials(raw);
    if !sensitive_query
        && parsed.username().is_empty()
        && parsed.password().is_none()
        && parsed.fragment().is_none()
    {
        return raw.into();
    }
    let _ = parsed.set_username("");
    let _ = parsed.set_password(None);
    parsed.set_fragment(None);
    if sensitive_query {
        let safe_pairs: Vec<_> = parsed
            .query_pairs()
            .into_owned()
            .filter(|(name, _)| !cache_url_query_parameter_is_sensitive(name))
            .collect();
        parsed.set_query(None);
        if !safe_pairs.is_empty() {
            parsed.query_pairs_mut().extend_pairs(safe_pairs);
        }
    }
    parsed.to_string()
}

/// Merges a URL without forwarding implicit credentials to a new identity.
///
/// Same-type omissions preserve the raw URL. An explicit URL equal to the whole
/// sanitized current URL also preserves raw credentials, so a GET/Save round
/// trip cannot erase server-only auth. Other explicit URLs use only their own
/// credentials. Type conversions strip credentials from inherited URLs.
pub(crate) fn effective_cache_url(
    current: Option<&str>,
    replacement: Option<&str>,
    same_type: bool,
) -> Option<String> {
    match (current, replacement) {
        (Some(current), Some(replacement)) if same_type => {
            let sanitized = sanitize_cache_url_credentials(current);
            let has_credentials = cache_url_has_query_credentials(current)
                || url::Url::parse(current)
                    .ok()
                    .is_some_and(|url| !url.username().is_empty() || url.password().is_some());
            let round_trip = has_credentials
                && sanitized != current
                && (replacement == sanitized
                    || (sanitized != "[REDACTED]"
                        && url::Url::parse(replacement)
                            .ok()
                            .zip(url::Url::parse(&sanitized).ok())
                            .is_some_and(|(replacement, sanitized)| replacement == sanitized)));
            Some(if round_trip { current } else { replacement }.into())
        }
        (_, Some(replacement)) => Some(replacement.into()),
        (Some(current), None) if same_type => Some(current.into()),
        (Some(current), None) => Some(sanitize_cache_url_credentials(current)),
        (None, None) => None,
    }
}

/// Explains why legacy credential-query probes are refused without replay.
pub(crate) const LEGACY_QUERY_CREDENTIALS_TEST_ERROR: &str = "Credential-query cache URLs cannot be tested. Migrate credentials to a supported authentication mechanism.";

fn validate_certificate_fields(fields: &[(&str, Option<&str>)]) -> Result<(), String> {
    for (field, value) in fields {
        if let Some(value) = value {
            crate::security::cache_secrets::validate_certificate_bundle(value)
                .map_err(|_| format!("{field} requires a certificate-only PEM bundle"))?;
        }
    }
    Ok(())
}

fn validate_write_auth(
    mode: Option<&str>,
    token: Option<&str>,
    cert: Option<&str>,
    key: Option<&str>,
    ca: Option<&str>,
) -> Result<(), String> {
    validate_certificate_fields(&[
        ("niks3_write_client_cert", cert),
        ("niks3_write_ca_cert", ca),
    ])?;
    match mode {
        Some("token") if nonempty(token) && cert.is_none() && key.is_none() && ca.is_none() => {
            Ok(())
        }
        Some("mtls") if token.is_none() && nonempty(cert) && nonempty(key) => Ok(()),
        _ => Err(
            "niks3_write_auth_mode requires token with only a token, or mtls with cert/key".into(),
        ),
    }
}

fn validate_read_auth(
    mode: Option<&str>,
    cert: Option<&str>,
    key: Option<&str>,
    ca: Option<&str>,
) -> Result<(), String> {
    validate_certificate_fields(&[("niks3_read_client_cert", cert), ("niks3_read_ca_cert", ca)])?;
    match mode {
        Some("none") if cert.is_none() && key.is_none() && ca.is_none() => Ok(()),
        Some("mtls") if nonempty(cert) && nonempty(key) => Ok(()),
        _ => Err(
            "niks3_read_auth_mode requires none without credentials, or mtls with cert/key".into(),
        ),
    }
}

impl CacheDestination {
    /// Rejects unsafe certificate fields before a stored model reaches an API.
    ///
    /// # Errors
    /// Returns a credential-free error for non-certificate PEM or invalid X.509.
    pub(crate) fn validate_niks3_certificates(&self) -> Result<(), String> {
        validate_certificate_fields(&[
            (
                "niks3_write_client_cert",
                self.niks3_write_client_cert.as_deref(),
            ),
            ("niks3_write_ca_cert", self.niks3_write_ca_cert.as_deref()),
            (
                "niks3_read_client_cert",
                self.niks3_read_client_cert.as_deref(),
            ),
            ("niks3_read_ca_cert", self.niks3_read_ca_cert.as_deref()),
        ])
    }
    /// Refreshes response flags from decrypted credentials, without DB columns.
    pub(crate) fn refresh_niks3_configured(&mut self) {
        self.http_basic_auth_configured = matches!(self.cache_type.as_str(), "Http" | "Nix")
            && self
                .push_to
                .as_deref()
                .and_then(|raw| url::Url::parse(raw).ok())
                .is_some_and(|url| {
                    matches!(url.scheme(), "https" | "http")
                        && (!url.username().is_empty() || url.password().is_some())
                });
        self.legacy_query_credentials_configured =
            [self.push_to.as_deref(), self.s3_endpoint_url.as_deref()]
                .into_iter()
                .flatten()
                .any(cache_url_has_query_credentials);
        self.attic_token_configured =
            self.cache_type == "Attic" && nonempty(self.attic_token.as_deref());
        self.s3_credentials_configured = self.cache_type == "S3"
            && nonempty(self.s3_access_key_id.as_deref())
            && nonempty(self.s3_secret_access_key.as_deref());
        self.s3_session_token_configured =
            self.cache_type == "S3" && nonempty(self.s3_session_token.as_deref());
        self.niks3_write_token_configured =
            self.cache_type == "Niks3" && nonempty(self.niks3_auth_token.as_deref());
        self.niks3_write_mtls_configured = self.cache_type == "Niks3"
            && nonempty(self.niks3_write_client_cert.as_deref())
            && nonempty(self.niks3_write_client_key.as_deref());
        self.niks3_read_mtls_configured = self.cache_type == "Niks3"
            && nonempty(self.niks3_read_client_cert.as_deref())
            && nonempty(self.niks3_read_client_key.as_deref());
    }

    /// Merges Niks3 updates before validation and persistence.
    ///
    /// Mode changes discard the previous mode's entire credential set. Fields
    /// omitted in an unchanged mode preserve existing credentials. Callers must
    /// validate the resulting configuration. Persistence callers must hold the DB
    /// row lock; read-only probes merge an unlocked point-in-time snapshot.
    ///
    /// # Errors
    /// Returns an error when a credential field is both replaced and cleared.
    pub(crate) fn merge_niks3_update(
        &self,
        update: &UpdateCacheDestination,
    ) -> Result<Self, String> {
        if (update.clear_niks3_auth_token && update.niks3_auth_token.is_some())
            || (update.clear_niks3_write_client_key && update.niks3_write_client_key.is_some())
            || (update.clear_niks3_read_client_key && update.niks3_read_client_key.is_some())
            || (update.clear_niks3_write_ca_cert && update.niks3_write_ca_cert.is_some())
            || (update.clear_niks3_read_ca_cert && update.niks3_read_ca_cert.is_some())
        {
            return Err("Cannot replace and clear the same Niks3 credential field".into());
        }
        let mut merged = self.clone();
        let leaves_niks3 = self.cache_type == "Niks3"
            && update.cache_type.as_deref().is_some_and(|ty| ty != "Niks3");
        if leaves_niks3
            || update
                .niks3_write_auth_mode
                .as_ref()
                .is_some_and(|mode| Some(mode) != self.niks3_write_auth_mode.as_ref())
        {
            merged.niks3_auth_token = None;
            merged.niks3_write_client_cert = None;
            merged.niks3_write_client_key = None;
            merged.niks3_write_ca_cert = None;
        }
        if leaves_niks3
            || update
                .niks3_read_auth_mode
                .as_ref()
                .is_some_and(|mode| Some(mode) != self.niks3_read_auth_mode.as_ref())
        {
            merged.niks3_read_client_cert = None;
            merged.niks3_read_client_key = None;
            merged.niks3_read_ca_cert = None;
        }
        macro_rules! replace {
            ($($field:ident),+) => {$(
                if let Some(value) = &update.$field {
                    merged.$field = Some(value.clone());
                }
            )+};
        }
        replace!(
            niks3_server_url,
            niks3_write_auth_mode,
            niks3_auth_token,
            niks3_write_client_cert,
            niks3_write_client_key,
            niks3_write_ca_cert,
            niks3_read_auth_mode,
            niks3_read_client_cert,
            niks3_read_client_key,
            niks3_read_ca_cert
        );
        if !update.niks3_public_keys.is_empty() {
            merged.niks3_public_keys = update.niks3_public_keys.clone();
        }
        if update.clear_niks3_auth_token {
            merged.niks3_auth_token = None;
        }
        if update.clear_niks3_write_client_key {
            merged.niks3_write_client_key = None;
        }
        if update.clear_niks3_read_client_key {
            merged.niks3_read_client_key = None;
        }
        if update.clear_niks3_write_ca_cert {
            merged.niks3_write_ca_cert = None;
        }
        if update.clear_niks3_read_ca_cert {
            merged.niks3_read_ca_cert = None;
        }
        merged.refresh_niks3_configured();
        Ok(merged)
    }

    /// Returns decrypted write authentication for a Niks3 destination.
    ///
    /// # Errors
    /// Returns an error for a different cache type or invalid write credentials.
    ///
    /// # Examples
    /// ```
    /// use crystal_forge::models::cache_destination::CacheDestination;
    /// use cf_protocol::cache::Niks3WriteAuth;
    /// let destination = CacheDestination {
    ///     cache_type: "Niks3".into(),
    ///     niks3_write_auth_mode: Some("token".into()),
    ///     niks3_auth_token: Some("example-token".into()),
    ///     ..Default::default()
    /// };
    /// assert!(matches!(destination.niks3_write_auth()?, Niks3WriteAuth::Token { .. }));
    /// # Ok::<(), String>(())
    /// ```
    pub fn niks3_write_auth(&self) -> Result<cf_protocol::cache::Niks3WriteAuth, String> {
        if self.cache_type != "Niks3" {
            return Err("Destination is not Niks3".into());
        }
        validate_write_auth(
            self.niks3_write_auth_mode.as_deref(),
            self.niks3_auth_token.as_deref(),
            self.niks3_write_client_cert.as_deref(),
            self.niks3_write_client_key.as_deref(),
            self.niks3_write_ca_cert.as_deref(),
        )?;
        match self.niks3_write_auth_mode.as_deref() {
            Some("token") => Ok(cf_protocol::cache::Niks3WriteAuth::Token {
                token: self.niks3_auth_token.clone().ok_or("Missing Niks3 token")?,
            }),
            _ => Ok(cf_protocol::cache::Niks3WriteAuth::Mtls {
                client_certificate: self
                    .niks3_write_client_cert
                    .clone()
                    .ok_or("Missing write certificate")?,
                client_private_key: self
                    .niks3_write_client_key
                    .clone()
                    .ok_or("Missing write key")?,
                ca_certificate: self.niks3_write_ca_cert.clone(),
            }),
        }
    }

    /// Returns the read URL, trusted signing keys, and decrypted read auth.
    ///
    /// Supports Niks3 and existing public cache types. Niks3's write token is
    /// never included. Callers must enforce confidential credential transport.
    ///
    /// # Errors
    /// Returns an error for missing URLs, unknown types, or invalid Niks3 reads.
    ///
    /// # Examples
    /// ```
    /// use crystal_forge::models::cache_destination::CacheDestination;
    /// use cf_protocol::cache::CacheReadAuth;
    /// let destination = CacheDestination {
    ///     cache_type: "Niks3".into(),
    ///     push_to: Some("https://cache.example.com".into()),
    ///     niks3_public_keys: vec!["cache-1:AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=".into()],
    ///     niks3_read_auth_mode: Some("none".into()),
    ///     ..Default::default()
    /// };
    /// let (url, keys, auth) = destination.read_config()?;
    /// assert_eq!(url, "https://cache.example.com");
    /// assert_eq!(keys.len(), 1);
    /// assert_eq!(auth, CacheReadAuth::None);
    /// # Ok::<(), String>(())
    /// ```
    pub fn read_config(
        &self,
    ) -> Result<(String, Vec<String>, cf_protocol::cache::CacheReadAuth), String> {
        let url = self
            .push_to
            .clone()
            .filter(|url| !url.trim().is_empty())
            .ok_or("Missing read URL")?;
        if self.cache_type != "Niks3" {
            if !matches!(self.cache_type.as_str(), "S3" | "Attic" | "Http" | "Nix") {
                return Err("Unknown cache type".into());
            }
            return Ok((
                url,
                self.attic_public_key.iter().cloned().collect(),
                cf_protocol::cache::CacheReadAuth::None,
            ));
        }
        validate_https(Some(&url), "push_to")?;
        if self.niks3_public_keys.is_empty() {
            return Err("niks3_public_keys requires nonempty signing keys".into());
        }
        for key in &self.niks3_public_keys {
            cf_protocol::cache::validate_nix_public_key(key).map_err(str::to_string)?;
        }
        validate_read_auth(
            self.niks3_read_auth_mode.as_deref(),
            self.niks3_read_client_cert.as_deref(),
            self.niks3_read_client_key.as_deref(),
            self.niks3_read_ca_cert.as_deref(),
        )?;
        let auth = if self.niks3_read_auth_mode.as_deref() == Some("mtls") {
            cf_protocol::cache::CacheReadAuth::Mtls {
                client_certificate: self
                    .niks3_read_client_cert
                    .clone()
                    .ok_or("Missing read certificate")?,
                client_private_key: self
                    .niks3_read_client_key
                    .clone()
                    .ok_or("Missing read key")?,
                ca_certificate: self.niks3_read_ca_cert.clone(),
            }
        } else {
            cf_protocol::cache::CacheReadAuth::None
        };
        Ok((url, self.niks3_public_keys.clone(), auth))
    }
}

/// Returns a format-valid public key for named cache fixtures, without secrets.
#[cfg(test)]
pub(crate) fn nix_public_key_fixture(name: &str) -> String {
    use base64::{Engine as _, engine::general_purpose::STANDARD};
    format!("{name}:{}", STANDARD.encode([0; 32]))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::security::cache_secrets::TEST_CERTIFICATE;

    fn niks3_create() -> CreateCacheDestination {
        CreateCacheDestination {
            name: "niks3".into(),
            cache_type: "Niks3".into(),
            push_to: Some("https://read.example.com/cache".into()),
            niks3_server_url: Some("https://write.example.com/api".into()),
            niks3_public_keys: vec![
                nix_public_key_fixture("cache-1"),
                nix_public_key_fixture("cache-2"),
            ],
            niks3_write_auth_mode: Some("token".into()),
            niks3_auth_token: Some("write-token-secret".into()),
            niks3_read_auth_mode: Some("none".into()),
            ..Default::default()
        }
    }

    #[test]
    fn niks3_validates_separate_https_planes_and_multiple_keys() {
        let mut create = niks3_create();
        create.validate().unwrap();
        for url in [
            "http://write.example.com",
            "file:///cache",
            "https://user:secret@example.com",
            "https://example.com/#fragment",
            "invalid",
        ] {
            create.niks3_server_url = Some(url.into());
            assert!(create.validate().is_err());
            create = niks3_create();
            create.push_to = Some(url.into());
            assert!(create.validate().is_err());
            create = niks3_create();
        }
        create.niks3_public_keys.clear();
        assert!(create.validate().is_err());
        create.niks3_public_keys = vec!["key".into(), "  ".into()];
        assert!(create.validate().is_err());
    }

    #[test]
    fn niks3_urls_reject_prepared_policy_query_names_without_echoing_credentials() {
        for name in [
            "token",
            "TOKEN",
            "ToKeN",
            "%74%6f%6b%65%6e",
            "auth-token",
            "AUTH-TOKEN",
            "%61uth%2Dtoken",
            "password",
            "PassWord",
            "access_token",
            "Access%5fToken",
            "ssl-verify",
            "SSL-CERT-FILE",
            "ca-certificate",
            "CA%2DCERTIFICATE",
            "tls-private-key",
            "TLS-CERTIFICATE",
            "%74ls%2dprivate%2dkey",
            "tls-anything",
        ] {
            let raw = format!(
                "https://cache.example.com/cache?priority=30&{name}=unique-url-secret&priority=40"
            );
            assert!(
                cf_config::cache_credentials::PreparedCacheRead::new(
                    &raw,
                    &[],
                    &cf_protocol::cache::CacheReadAuth::None,
                )
                .is_err()
            );
            for field in ["niks3_server_url", "push_to"] {
                let mut json = serde_json::to_value(niks3_create()).unwrap();
                json[field] = serde_json::Value::String(raw.clone());
                let create: CreateCacheDestination = serde_json::from_value(json).unwrap();
                let error = create.validate().unwrap_err();
                assert!(error.contains(field));
                assert!(!error.contains("unique-url-secret"));
                assert!(!error.contains(&raw));
            }
            let read = CacheDestination {
                cache_type: "Niks3".into(),
                push_to: Some(raw),
                niks3_public_keys: vec![nix_public_key_fixture("cache")],
                niks3_read_auth_mode: Some("none".into()),
                ..Default::default()
            };
            assert!(
                !read
                    .read_config()
                    .unwrap_err()
                    .contains("unique-url-secret")
            );
        }
    }

    #[test]
    fn niks3_safe_queries_remain_allowed_and_generic_urls_keep_their_policy() {
        let mut create = niks3_create();
        create.push_to = Some("https://read.example.com?priority=30&region=us-east-1".into());
        create.niks3_server_url = Some("https://write.example.com?region=us-east-1".into());
        create.validate().unwrap();
        for raw in [
            "https://user:unique-url-secret@cache.example.com",
            "https://cache.example.com/#unique-url-secret",
            "https://cache.example.com/?priority=30\n",
            "https://@cache.example.com",
        ] {
            assert!(validate_https(Some(raw), "push_to").is_err());
            assert!(
                !validate_https(Some(raw), "push_to")
                    .unwrap_err()
                    .contains("unique-url-secret")
            );
        }
        CreateCacheDestination {
            name: "generic".into(),
            cache_type: "Nix".into(),
            push_to: Some("https://cache.example.com?token=generic-token".into()),
            ..Default::default()
        }
        .validate()
        .unwrap();
    }

    #[test]
    fn niks3_auth_validation_rejects_incomplete_and_mixed_modes() {
        let mut create = niks3_create();
        create.niks3_auth_token = Some("  ".into());
        assert!(create.validate().is_err());
        create = niks3_create();
        create.niks3_write_client_cert = Some(TEST_CERTIFICATE.into());
        assert!(create.validate().is_err());
        create.niks3_write_auth_mode = Some("mtls".into());
        create.niks3_write_client_key = Some("write-key".into());
        assert!(create.validate().is_err());
        create.niks3_auth_token = None;
        create.validate().unwrap();
        create.niks3_read_client_cert = Some(TEST_CERTIFICATE.into());
        assert!(create.validate().is_err());
        create.niks3_read_auth_mode = Some("mtls".into());
        assert!(create.validate().is_err());
        create.niks3_read_client_key = Some("read-key".into());
        create.validate().unwrap();
        for invalid_mode in ["bearer", "oidc", "", "MTLS"] {
            create.niks3_read_auth_mode = Some(invalid_mode.into());
            assert!(create.validate().is_err());
            create.niks3_read_auth_mode = Some("mtls".into());
            create.niks3_write_auth_mode = Some(invalid_mode.into());
            assert!(create.validate().is_err());
            create.niks3_write_auth_mode = Some("mtls".into());
        }
    }

    #[test]
    fn defaults_accept_legacy_requests_and_debug_redacts_new_secrets() {
        let create: CreateCacheDestination = serde_json::from_value(serde_json::json!({
            "name": "public", "cache_type": "Nix", "push_to": "https://cache.example.com"
        }))
        .unwrap();
        assert!(create.niks3_public_keys.is_empty());
        create.validate().unwrap();
        let mut update: UpdateCacheDestination = serde_json::from_str("{}").unwrap();
        assert!(!update.clear_niks3_auth_token);
        assert!(!update.clear_niks3_write_client_key);
        assert!(!update.clear_niks3_read_client_key);
        assert!(!update.clear_niks3_write_ca_cert);
        assert!(!update.clear_niks3_read_ca_cert);
        update.niks3_auth_token = Some("token-secret".into());
        update.niks3_write_client_key = Some("write-key-secret".into());
        update.niks3_read_client_key = Some("read-key-secret".into());
        let destination = CacheDestination {
            niks3_auth_token: update.niks3_auth_token.clone(),
            niks3_write_client_key: update.niks3_write_client_key.clone(),
            niks3_read_client_key: update.niks3_read_client_key.clone(),
            ..Default::default()
        };
        for debug in [
            format!("{update:?}"),
            format!("{destination:?}"),
            format!("{:?}", niks3_create()),
        ] {
            for secret in [
                "token-secret",
                "write-key-secret",
                "read-key-secret",
                "write-token-secret",
            ] {
                assert!(!debug.contains(secret));
            }
        }
        let json = serde_json::to_value(destination).unwrap();
        assert!(json.get("niks3_auth_token").is_none());
        assert!(json.get("niks3_write_client_key").is_none());
        assert!(json.get("niks3_read_client_key").is_none());
    }

    #[test]
    fn protocol_helpers_keep_write_credentials_out_of_read_plane() {
        let mut destination = CacheDestination {
            cache_type: "Niks3".into(),
            push_to: Some("https://read.example.com/cache".into()),
            niks3_public_keys: vec![nix_public_key_fixture("one"), nix_public_key_fixture("two")],
            niks3_write_auth_mode: Some("token".into()),
            niks3_auth_token: Some("write-token".into()),
            niks3_read_auth_mode: Some("none".into()),
            ..Default::default()
        };
        assert_eq!(
            destination.niks3_write_auth().unwrap(),
            cf_protocol::cache::Niks3WriteAuth::Token {
                token: "write-token".into()
            }
        );
        let (_, keys, auth) = destination.read_config().unwrap();
        assert_eq!(keys.len(), 2);
        assert_eq!(auth, cf_protocol::cache::CacheReadAuth::None);
        destination.niks3_read_auth_mode = Some("mtls".into());
        destination.niks3_read_client_cert = Some(TEST_CERTIFICATE.into());
        assert!(destination.read_config().is_err());
        destination.niks3_read_client_key = Some("read-key".into());
        destination.refresh_niks3_configured();
        assert!(destination.niks3_write_token_configured);
        assert!(destination.niks3_read_mtls_configured);
        assert!(!destination.niks3_write_mtls_configured);
        assert_eq!(
            destination.read_config().unwrap().2,
            cf_protocol::cache::CacheReadAuth::Mtls {
                client_certificate: TEST_CERTIFICATE.into(),
                client_private_key: "read-key".into(),
                ca_certificate: None,
            }
        );
    }

    #[test]
    fn all_certificate_fields_reject_combined_pem_and_arbitrary_data() {
        let mut valid = niks3_create();
        valid.niks3_write_auth_mode = Some("mtls".into());
        valid.niks3_auth_token = None;
        valid.niks3_write_client_cert = Some(TEST_CERTIFICATE.into());
        valid.niks3_write_client_key = Some("encrypted-write-key-input".into());
        valid.niks3_write_ca_cert = Some(TEST_CERTIFICATE.into());
        valid.niks3_read_auth_mode = Some("mtls".into());
        valid.niks3_read_client_cert = Some(TEST_CERTIFICATE.into());
        valid.niks3_read_client_key = Some("encrypted-read-key-input".into());
        valid.niks3_read_ca_cert = Some(TEST_CERTIFICATE.into());
        valid.validate().unwrap();
        for field in [
            "niks3_write_client_cert",
            "niks3_write_ca_cert",
            "niks3_read_client_cert",
            "niks3_read_ca_cert",
        ] {
            for bad in [
                format!(
                    "{TEST_CERTIFICATE}-----BEGIN PRIVATE KEY-----\nAQID\n-----END PRIVATE KEY-----"
                ),
                format!(
                    "-----BEGIN RSA PRIVATE KEY-----\nAQID\n-----END RSA PRIVATE KEY-----\n{TEST_CERTIFICATE}"
                ),
                "arbitrary secret data".into(),
                format!("{TEST_CERTIFICATE}unframed-secret"),
                "-----BEGIN CERTIFICATE-----\nAQID\n-----END CERTIFICATE-----".into(),
                String::new(),
            ] {
                let mut json = serde_json::to_value(&valid).unwrap();
                json[field] = serde_json::Value::String(bad);
                let create: CreateCacheDestination = serde_json::from_value(json).unwrap();
                let error = create.validate().unwrap_err();
                assert!(error.contains(field));
                assert!(!error.contains("unframed-secret"));
                // Non-Niks3 fields are persisted too, so they need the same gate.
                let mut other = create;
                other.cache_type = "Nix".into();
                assert!(other.validate().is_err());
            }
        }
    }

    #[test]
    fn test_validate_attic_requires_cache_name() {
        let create = CreateCacheDestination {
            name: "test".to_string(),
            cache_type: "Attic".to_string(),
            push_to: Some("https://attic.example.com".to_string()),
            attic_cache_name: None,
            attic_public_key: None,
            enabled: None,
            signing_key_path: None,
            compression: None,
            s3_region: None,
            s3_profile: None,
            s3_access_key_id: None,
            s3_secret_access_key: None,
            s3_session_token: None,
            s3_endpoint_url: None,
            attic_token: None,
            attic_ignore_upstream_cache_filter: None,
            attic_jobs: None,
            parallel_uploads: None,
            max_retries: None,
            retry_delay_seconds: None,
            push_timeout_seconds: None,
            force_repush: None,
            require_sigs: None,
            environment_ids: None,
            ..Default::default()
        };

        let result = create.validate();
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("attic_cache_name"));
    }

    #[test]
    fn test_validate_attic_requires_token() {
        let create = CreateCacheDestination {
            name: "test".to_string(),
            cache_type: "Attic".to_string(),
            push_to: Some("https://attic.example.com".to_string()),
            attic_cache_name: Some("my-cache".to_string()),
            attic_public_key: Some("cache.example.com-1:abc123".to_string()),
            enabled: None,
            signing_key_path: None,
            compression: None,
            s3_region: None,
            s3_profile: None,
            s3_access_key_id: None,
            s3_secret_access_key: None,
            s3_session_token: None,
            s3_endpoint_url: None,
            attic_token: None,
            attic_ignore_upstream_cache_filter: None,
            attic_jobs: None,
            parallel_uploads: None,
            max_retries: None,
            retry_delay_seconds: None,
            push_timeout_seconds: None,
            force_repush: None,
            require_sigs: None,
            environment_ids: None,
            ..Default::default()
        };

        let result = create.validate();
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("attic_token"));
    }

    #[test]
    fn test_validate_s3_requires_push_to() {
        let create = CreateCacheDestination {
            name: "test".to_string(),
            cache_type: "S3".to_string(),
            push_to: None,
            attic_cache_name: None,
            attic_public_key: None,
            enabled: None,
            signing_key_path: None,
            compression: None,
            s3_region: None,
            s3_profile: None,
            s3_access_key_id: None,
            s3_secret_access_key: None,
            s3_session_token: None,
            s3_endpoint_url: None,
            attic_token: Some("token".to_string()),
            attic_ignore_upstream_cache_filter: None,
            attic_jobs: None,
            parallel_uploads: None,
            max_retries: None,
            retry_delay_seconds: None,
            push_timeout_seconds: None,
            force_repush: None,
            require_sigs: None,
            environment_ids: None,
            ..Default::default()
        };

        let result = create.validate();
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("push_to"));
    }

    #[test]
    fn test_validate_s3_requires_access_key_id() {
        let create = CreateCacheDestination {
            name: "test".to_string(),
            cache_type: "S3".to_string(),
            push_to: Some("s3://my-bucket".to_string()),
            attic_cache_name: None,
            attic_public_key: None,
            enabled: None,
            signing_key_path: None,
            compression: None,
            s3_region: Some("us-east-1".to_string()),
            s3_profile: None,
            s3_access_key_id: None,
            s3_secret_access_key: Some("secret".to_string()),
            s3_session_token: None,
            s3_endpoint_url: Some("https://s3.us-east-1.amazonaws.com".to_string()),
            attic_token: Some("token".to_string()),
            attic_ignore_upstream_cache_filter: None,
            attic_jobs: None,
            parallel_uploads: None,
            max_retries: None,
            retry_delay_seconds: None,
            push_timeout_seconds: None,
            force_repush: None,
            require_sigs: None,
            environment_ids: None,
            ..Default::default()
        };

        let result = create.validate();
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("s3_access_key_id"));
    }

    #[test]
    fn test_validate_attic_succeeds_with_cache_name() {
        let create = CreateCacheDestination {
            name: "test-attic".to_string(),
            cache_type: "Attic".to_string(),
            push_to: Some("https://attic.example.com".to_string()),
            attic_cache_name: Some("my-cache".to_string()),
            attic_public_key: Some("cache.example.com-1:abc123".to_string()),
            enabled: None,
            signing_key_path: None,
            compression: None,
            s3_region: None,
            s3_profile: None,
            s3_access_key_id: None,
            s3_secret_access_key: None,
            s3_session_token: None,
            s3_endpoint_url: None,
            attic_token: Some("token".to_string()),
            attic_ignore_upstream_cache_filter: None,
            attic_jobs: None,
            parallel_uploads: None,
            max_retries: None,
            retry_delay_seconds: None,
            push_timeout_seconds: None,
            force_repush: None,
            require_sigs: None,
            environment_ids: None,
            ..Default::default()
        };

        let result = create.validate();
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_rejects_invalid_cache_type() {
        let create = CreateCacheDestination {
            name: "test".to_string(),
            cache_type: "InvalidType".to_string(),
            push_to: None,
            attic_cache_name: None,
            attic_public_key: None,
            enabled: None,
            signing_key_path: None,
            compression: None,
            s3_region: None,
            s3_profile: None,
            s3_access_key_id: None,
            s3_secret_access_key: None,
            s3_session_token: None,
            s3_endpoint_url: None,
            attic_token: None,
            attic_ignore_upstream_cache_filter: None,
            attic_jobs: None,
            parallel_uploads: None,
            max_retries: None,
            retry_delay_seconds: None,
            push_timeout_seconds: None,
            force_repush: None,
            require_sigs: None,
            environment_ids: None,
            ..Default::default()
        };

        let result = create.validate();
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Invalid cache_type"));
    }

    #[test]
    fn test_validate_rejects_empty_name() {
        let create = CreateCacheDestination {
            name: "   ".to_string(),
            cache_type: "Nix".to_string(),
            push_to: Some("https://cache.example.com".to_string()),
            attic_cache_name: None,
            attic_public_key: None,
            enabled: None,
            signing_key_path: None,
            compression: None,
            s3_region: None,
            s3_profile: None,
            s3_access_key_id: None,
            s3_secret_access_key: None,
            s3_session_token: None,
            s3_endpoint_url: None,
            attic_token: None,
            attic_ignore_upstream_cache_filter: None,
            attic_jobs: None,
            parallel_uploads: None,
            max_retries: None,
            retry_delay_seconds: None,
            push_timeout_seconds: None,
            force_repush: None,
            require_sigs: None,
            environment_ids: None,
            ..Default::default()
        };

        let result = create.validate();
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("name cannot be empty"));
    }
}
