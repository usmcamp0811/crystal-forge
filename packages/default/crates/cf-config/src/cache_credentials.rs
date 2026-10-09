//! Prepares protected, process-scoped credentials for Niks3 writes and Nix reads.
//!
//! Keep each prepared value alive until all consuming child processes exit,
//! including cancellation and timeout cleanup. Dropping the value removes its
//! temporary files; cleanup unlinks files and does not promise secure erasure.
//! These helpers validate transport syntax, not DNS targets or SSRF policy.

use anyhow::{Result, anyhow, ensure};
use cf_protocol::cache::{CacheReadAuth, Niks3WriteAuth};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::PathBuf;
use tempfile::TempDir;
use url::Url;

/// Names the native Nix setting that binds netrc reads to one HTTPS origin.
///
/// The patched native implementation matches scheme, normalized host and port
/// exactly and disables all redirects while this setting is active. Unpatched
/// Nix may ignore unknown settings, so consumers must probe support first.
pub const CF_NETRC_AUTHORITY_SETTING: &str = "cf-netrc-authority";

/// Records nonsecret feature evidence from the selected runtime Nix executable.
///
/// Consumers must obtain this evidence from a successful `nix config show --json`
/// (or equivalent `show-config --json`) invocation of the same executable used
/// for the read. Do not infer support from a version, package name, environment
/// variable, configured option or the agent's signed capability alone.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct NixReadFeatures {
    supports_netrc_authority: bool,
}

impl NixReadFeatures {
    /// Parses runtime settings output without creating files or spawning a child.
    ///
    /// A registered string-valued `cf-netrc-authority` setting establishes support.
    /// Its current value is not used as the authorized read origin.
    ///
    /// # Errors
    /// Returns a static error for malformed JSON, a non-object settings document,
    /// or a malformed guard setting. Errors never include probe output.
    ///
    /// # Examples
    /// ```
    /// use cf_config::cache_credentials::NixReadFeatures;
    /// let features = NixReadFeatures::from_settings_json(
    ///     br#"{"cf-netrc-authority":{"value":""}}"#,
    /// )?;
    /// assert!(features.supports_netrc_authority());
    /// # Ok::<(), anyhow::Error>(())
    /// ```
    pub fn from_settings_json(output: &[u8]) -> Result<Self> {
        let settings: serde_json::Value = serde_json::from_slice(output)
            .map_err(|_| anyhow!("invalid Nix runtime settings output"))?;
        let settings = settings
            .as_object()
            .ok_or_else(|| anyhow!("invalid Nix runtime settings output"))?;
        let supports_netrc_authority = match settings.get(CF_NETRC_AUTHORITY_SETTING) {
            None => false,
            Some(setting) => {
                ensure!(
                    setting
                        .get("value")
                        .is_some_and(serde_json::Value::is_string),
                    "invalid Nix native netrc guard setting"
                );
                true
            }
        };
        Ok(Self {
            supports_netrc_authority,
        })
    }

    /// Returns whether the probed runtime exposes the native netrc origin guard.
    pub fn supports_netrc_authority(&self) -> bool {
        self.supports_netrc_authority
    }
}

/// Owns a Niks3 push command and the credential files consumed by that command.
///
/// Arguments contain file paths, never token or private-key contents. Keep the
/// owner alive through child termination. Consumers must clear ambient
/// `NIKS3_AUTH_TOKEN_FILE` and isolate `XDG_CONFIG_HOME`/`HOME` for mTLS commands:
/// Niks3 checks ambient token files before selecting certificate-only auth.
pub struct PreparedNiks3Push {
    /// Executable name resolved by the consumer's packaged PATH.
    pub command: String,
    /// Arguments passed directly to the executable, without a shell.
    pub args: Vec<String>,
    _credentials: TempDir,
}

impl std::fmt::Debug for PreparedNiks3Push {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("PreparedNiks3Push([REDACTED])")
    }
}

impl PreparedNiks3Push {
    /// Prepares one HTTPS push with protected token or mTLS files.
    ///
    /// Zero parallel uploads selects one upload, matching the Niks3 CLI's lower
    /// bound. The limit is passed once for this process, not multiplied by jobs.
    ///
    /// # Errors
    /// Returns a credential-free error for an invalid URL, empty credential,
    /// non-canonical store path, or failure to create protected temporary files.
    /// PEM validity and remote authorization are checked by the consuming CLI.
    ///
    /// # Examples
    /// ```no_run
    /// use cf_config::cache_credentials::PreparedNiks3Push;
    /// use cf_protocol::cache::Niks3WriteAuth;
    /// let auth = Niks3WriteAuth::Token { token: "secret".into() };
    /// let prepared = PreparedNiks3Push::new(
    ///     "https://writes.example.org", &auth, 2, "/nix/store/abc-output",
    /// )?;
    /// let status = std::process::Command::new(&prepared.command)
    ///     .args(&prepared.args).status()?;
    /// drop(prepared); // The child has exited; credentials can be removed.
    /// # Ok::<(), anyhow::Error>(())
    /// ```
    pub fn new(
        server_url: &str,
        auth: &Niks3WriteAuth,
        parallel_uploads: u32,
        store_path: &str,
    ) -> Result<Self> {
        let url = validated_url(server_url, true)?;
        ensure!(
            cf_protocol::builder::is_canonical_nix_store_path(store_path, false),
            "push requires a canonical Nix store path"
        );
        let credentials = protected_directory()?;
        let mut args = vec![
            "push".into(),
            "--server-url".into(),
            url.to_string(),
            "--max-concurrent-uploads".into(),
            parallel_uploads.max(1).to_string(),
        ];
        match auth {
            Niks3WriteAuth::Token { token } => {
                let path = protected_file(&credentials, "token", token)?;
                args.extend(["--auth-token-path".into(), path_string(&path)?]);
            }
            Niks3WriteAuth::Mtls {
                client_certificate,
                client_private_key,
                ca_certificate,
            } => {
                let cert = protected_file(&credentials, "client-cert.pem", client_certificate)?;
                let key = protected_file(&credentials, "client-key.pem", client_private_key)?;
                args.extend([
                    "--client-cert".into(),
                    path_string(&cert)?,
                    "--client-key".into(),
                    path_string(&key)?,
                ]);
                if let Some(ca) = ca_certificate {
                    let path = protected_file(&credentials, "ca.pem", ca)?;
                    args.extend(["--ca-cert".into(), path_string(&path)?]);
                }
            }
        }
        args.extend(["--".into(), store_path.into()]);
        Ok(Self {
            command: "niks3".into(),
            args,
            _credentials: credentials,
        })
    }
}

/// Owns read-plane Nix settings and optional protected credential files.
///
/// mTLS URLs reference files through encoded Nix store parameters. Basic URLs
/// contain no authentication; explicit Nix options select the protected netrc
/// and native origin guard. Consumers must keep this owner alive until Nix exits
/// and preserve signature verification when applying the public keys.
pub struct PreparedCacheRead {
    /// Credential-free substituter URL with managed mTLS file parameters if used.
    pub url: String,
    /// Space-separated keys for Nix's `trusted-public-keys` option.
    pub trusted_public_keys: String,
    /// Optional trust bundle to apply through child-local `NIX_SSL_CERT_FILE`.
    /// Consumers must not replace the parent process's environment globally.
    pub ca_certificate_path: Option<PathBuf>,
    /// Explicit child-local Nix options, containing only public values and paths.
    ///
    /// Includes mandatory signatures and extra signing keys, preserving the
    /// parent's trust list. Basic additionally sets netrc-file and the exact
    /// authorized HTTPS origin; consumers must apply every entry as argv.
    pub nix_settings: Vec<(String, String)>,
    /// Requires native origin/redirect protection for this prepared Basic read.
    ///
    /// Preparation already checked runtime feature evidence. Consumers must use
    /// that same executable and apply all settings for every read child.
    pub basic_guard_required: bool,
    _credentials: Option<TempDir>,
}

impl std::fmt::Debug for PreparedCacheRead {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("PreparedCacheRead([REDACTED])")
    }
}

impl PreparedCacheRead {
    /// Prepares public HTTP(S) reads or mTLS HTTPS reads.
    ///
    /// Existing non-credential query parameters are preserved. Keys are joined
    /// in input order; empty or whitespace-containing key entries are rejected
    /// to prevent accidental changes to Nix's space-separated trust list.
    ///
    /// # Errors
    /// Returns a credential-free error for invalid URLs, insecure authenticated
    /// transport, invalid key entries, empty credentials, or temporary-file I/O.
    /// Basic fails closed before creating files; use [`Self::new_with_nix_features`]
    /// after probing the runtime. PEM and signing-key validity are checked by Nix.
    ///
    /// # Examples
    /// ```
    /// use cf_config::cache_credentials::PreparedCacheRead;
    /// use cf_protocol::cache::CacheReadAuth;
    /// let read = PreparedCacheRead::new(
    ///     "https://cache.example.org", &["cache:public-key".into()],
    ///     &CacheReadAuth::None,
    /// )?;
    /// assert_eq!(read.trusted_public_keys, "cache:public-key");
    /// # Ok::<(), anyhow::Error>(())
    /// ```
    pub fn new(url: &str, keys: &[String], auth: &CacheReadAuth) -> Result<Self> {
        Self::new_with_nix_features(url, keys, auth, &NixReadFeatures::default())
    }

    /// Prepares a read after checking nonsecret evidence from the runtime Nix.
    ///
    /// Basic requires native exact-origin/redirect protection and nonempty signing
    /// keys. Its URL remains credential-free. The netrc contains one quoted host
    /// record, never a default record. Keep this owner alive through child reap,
    /// including cancellation; apply [`Self::apply_to_nix_command`] to each child.
    /// The caller supplies the authorized URL/credential snapshot and owns DNS,
    /// SSRF policy, runtime probe execution and executable identity.
    ///
    /// # Errors
    /// Returns static errors for unavailable native protection, invalid URL or
    /// keys, empty credentials, control characters, a colon in the login name,
    /// or protected temporary-file I/O failure. No credentials are written when
    /// runtime protection is absent or Basic input validation fails.
    ///
    /// # Examples
    /// ```no_run
    /// use cf_config::cache_credentials::{NixReadFeatures, PreparedCacheRead};
    /// use cf_protocol::cache::CacheReadAuth;
    /// let probe = std::process::Command::new("nix")
    ///     .args(["config", "show", "--json"]).output()?;
    /// anyhow::ensure!(probe.status.success(), "Nix feature probe failed");
    /// let features = NixReadFeatures::from_settings_json(&probe.stdout)?;
    /// let read = PreparedCacheRead::new_with_nix_features(
    ///     "https://cache.example.org", &["cache:public-key".into()],
    ///     &CacheReadAuth::Basic { username: "login".into(), password: "secret".into() },
    ///     &features,
    /// )?;
    /// let mut command = std::process::Command::new("nix");
    /// command.args(["copy", "--from", &read.url, "/nix/store/abc-output"]);
    /// read.apply_to_nix_command(&mut command);
    /// let status = command.status()?;
    /// drop(read); // All consuming children have exited.
    /// # Ok::<(), anyhow::Error>(())
    /// ```
    pub fn new_with_nix_features(
        url: &str,
        keys: &[String],
        auth: &CacheReadAuth,
        features: &NixReadFeatures,
    ) -> Result<Self> {
        let basic_guard_required = matches!(auth, CacheReadAuth::Basic { .. });
        ensure!(
            !basic_guard_required || features.supports_netrc_authority(),
            "Basic cache reads require verified native cf-netrc-authority support"
        );
        let mut url = validated_url(url, !matches!(auth, CacheReadAuth::None))?;
        ensure!(
            keys.iter()
                .all(|key| !key.is_empty()
                    && !key.chars().any(|c| c.is_whitespace() || c.is_control())),
            "invalid cache public key entry"
        );
        ensure!(
            !basic_guard_required || !keys.is_empty(),
            "Basic cache reads require signing keys"
        );
        let mut nix_settings = vec![("require-sigs".into(), "true".into())];
        if !keys.is_empty() {
            nix_settings.push(("extra-trusted-public-keys".into(), keys.join(" ")));
        }
        let mut ca_certificate_path = None;
        let credentials = match auth {
            CacheReadAuth::None => None,
            CacheReadAuth::Basic { username, password } => {
                validate_basic_credentials(username, password)?;
                let host = match url.host() {
                    Some(url::Host::Ipv6(host)) => host.to_string(),
                    Some(host) => host.to_string(),
                    None => return Err(anyhow!("cache URL requires a host")),
                };
                let contents = format!(
                    "machine {} login {} password {}\n",
                    quote_netrc(&host),
                    quote_netrc(username),
                    quote_netrc(password)
                );
                let directory = protected_directory()?;
                let netrc = protected_file(&directory, "netrc", &contents)?;
                nix_settings.push(("netrc-file".into(), path_string(&netrc)?));
                nix_settings.push((
                    CF_NETRC_AUTHORITY_SETTING.into(),
                    url.origin().ascii_serialization(),
                ));
                Some(directory)
            }
            CacheReadAuth::Mtls {
                client_certificate,
                client_private_key,
                ca_certificate,
            } => {
                let directory = protected_directory()?;
                let cert = protected_file(&directory, "client-cert.pem", client_certificate)?;
                let key = protected_file(&directory, "client-key.pem", client_private_key)?;
                url.query_pairs_mut()
                    .append_pair("tls-certificate", &path_string(&cert)?)
                    .append_pair("tls-private-key", &path_string(&key)?);
                if let Some(ca) = ca_certificate {
                    ca_certificate_path = Some(protected_file(&directory, "ca.pem", ca)?);
                }
                Some(directory)
            }
        };
        Ok(Self {
            url: url.into(),
            trusted_public_keys: keys.join(" "),
            ca_certificate_path,
            nix_settings,
            basic_guard_required,
            _credentials: credentials,
        })
    }

    /// Applies all read settings and child-local TLS trust to a native command.
    ///
    /// Uses argv directly, never shell text or credential contents. Extra keys
    /// preserve parent trust and signatures are mandatory. Apply after any other
    /// Nix options so a consumer does not override protection accidentally. For
    /// Tokio commands, pass `command.as_std_mut()`. Keep this owner until every
    /// consuming child is terminated and reaped; this method does not transfer
    /// credential ownership or choose the substituter/store operation.
    pub fn apply_to_nix_command(&self, command: &mut std::process::Command) {
        for (name, value) in &self.nix_settings {
            command.args(["--option", name, value]);
        }
        if let Some(ca) = &self.ca_certificate_path {
            command.env("NIX_SSL_CERT_FILE", ca);
        }
    }
}

fn validate_basic_credentials(username: &str, password: &str) -> Result<()> {
    ensure!(
        !username.trim().is_empty() && !password.trim().is_empty(),
        "Basic cache credentials must not be empty"
    );
    ensure!(
        !username
            .chars()
            .chain(password.chars())
            .any(char::is_control),
        "Basic cache credentials must not contain control characters"
    );
    // SECURITY: HTTP Basic uses the first colon to separate login and password.
    ensure!(
        !username.contains(':'),
        "Basic cache login must not contain a colon"
    );
    Ok(())
}

fn quote_netrc(value: &str) -> String {
    // COMPATIBILITY: libcurl 8.21 netrc quoted tokens escape backslash and quote.
    // Always quote tokens so spaces and netrc keywords cannot inject fields.
    format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
}

fn validated_url(value: &str, require_https: bool) -> Result<Url> {
    // SECURITY: Never attach raw URLs or parser input to errors. Userinfo and
    // caller-supplied TLS parameters must not bypass managed credentials.
    ensure!(
        !value.chars().any(|c| c.is_whitespace() || c.is_control()),
        "invalid cache URL"
    );
    let url = Url::parse(value).map_err(|_| anyhow!("invalid cache URL"))?;
    ensure!(
        matches!(url.scheme(), "http" | "https") && url.host_str().is_some(),
        "cache URL requires HTTP or HTTPS and a host"
    );
    let authority = value
        .split_once("://")
        .map(|(_, rest)| rest.split(['/', '?', '#']).next().unwrap_or_default());
    ensure!(
        url.username().is_empty()
            && url.password().is_none()
            && !authority.is_some_and(|a| a.contains('@')),
        "cache URL must not contain embedded credentials"
    );
    ensure!(
        url.fragment().is_none(),
        "cache URL must not contain a fragment"
    );
    ensure!(
        !url.query_pairs().any(|(key, _)| {
            let key = key.to_ascii_lowercase();
            key.starts_with("tls-")
                || matches!(
                    key.as_str(),
                    "ssl-verify"
                        | "ssl-cert-file"
                        | "ca-certificate"
                        | "netrc-file"
                        | CF_NETRC_AUTHORITY_SETTING
                        | "token"
                        | "auth-token"
                        | "password"
                        | "access_token"
                )
        }),
        "cache URL must not contain credential parameters"
    );
    ensure!(
        !require_https || url.scheme() == "https",
        "cache credentials require HTTPS"
    );
    Ok(url)
}

fn protected_directory() -> Result<TempDir> {
    let directory = tempfile::Builder::new()
        .prefix("cf-cache-")
        .permissions(fs::Permissions::from_mode(0o700))
        .tempdir()
        .map_err(|_| anyhow!("could not create cache credential directory"))?;
    Ok(directory)
}

fn protected_file(directory: &TempDir, name: &str, contents: &str) -> Result<PathBuf> {
    ensure!(
        !contents.trim().is_empty(),
        "cache credential must not be empty"
    );
    let path = directory.path().join(name);
    // SECURITY: Fixed names in an owner-only directory and create_new prevent
    // replacement of existing files. Restrict permissions before writing bytes.
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&path)
        .map_err(|_| anyhow!("could not create cache credential file"))?;
    file.set_permissions(fs::Permissions::from_mode(0o600))
        .map_err(|_| anyhow!("could not restrict cache credential file"))?;
    file.write_all(contents.as_bytes())
        .map_err(|_| anyhow!("could not write cache credential file"))?;
    Ok(path)
}

fn path_string(path: &std::path::Path) -> Result<String> {
    path.to_str()
        .map(str::to_owned)
        .ok_or_else(|| anyhow!("cache credential path must be UTF-8"))
}

#[cfg(test)]
mod tests {
    use super::*;

    const CERT: &str = "unique-client-certificate";
    const KEY: &str = "unique-private-key";
    const CA: &str = "unique-ca-certificate";
    const TOKEN: &str = "unique-secret-token";
    const STORE: &str = "/nix/store/abc-output";

    fn guard_features() -> NixReadFeatures {
        NixReadFeatures::from_settings_json(br#"{"cf-netrc-authority":{"value":""}}"#).unwrap()
    }

    fn basic(username: &str, password: &str) -> CacheReadAuth {
        CacheReadAuth::Basic {
            username: username.into(),
            password: password.into(),
        }
    }

    #[test]
    fn basic_guard_probe_fails_closed_for_old_or_malformed_runtime() {
        for output in [b"{}".as_slice(), br#"{"netrc-file":{"value":"/ambient"}}"#] {
            let features = NixReadFeatures::from_settings_json(output).unwrap();
            assert!(!features.supports_netrc_authority());
            assert!(
                PreparedCacheRead::new_with_nix_features(
                    "https://cache.example",
                    &["cache:key".into()],
                    &basic("login", "password"),
                    &features
                )
                .is_err()
            );
        }
        for malformed in [
            b"not-json".as_slice(),
            b"[]",
            br#"{"cf-netrc-authority":true}"#,
            br#"{"cf-netrc-authority":{"value":false}}"#,
        ] {
            assert!(NixReadFeatures::from_settings_json(malformed).is_err());
        }
        assert!(
            PreparedCacheRead::new(
                "https://cache.example",
                &["cache:key".into()],
                &basic("login", "password")
            )
            .is_err()
        );
        assert!(guard_features().supports_netrc_authority());
    }

    #[test]
    fn basic_netrc_is_quoted_protected_origin_bound_and_owned_until_drop() {
        let username = "private login \\\" machine attacker";
        let password = "private password \\\" default login injected:colon";
        let read = PreparedCacheRead::new_with_nix_features(
            "https://READ.Example:443/cache?priority=30",
            &["one:key".into(), "two:key".into()],
            &basic(username, password),
            &guard_features(),
        )
        .unwrap();
        assert!(read.basic_guard_required);
        assert_eq!(read.url, "https://read.example/cache?priority=30");
        assert_eq!(read.trusted_public_keys, "one:key two:key");
        assert!(read.ca_certificate_path.is_none());
        let settings: std::collections::HashMap<_, _> = read.nix_settings.iter().cloned().collect();
        assert_eq!(settings[CF_NETRC_AUTHORITY_SETTING], "https://read.example");
        assert_eq!(settings["require-sigs"], "true");
        assert_eq!(settings["extra-trusted-public-keys"], "one:key two:key");
        assert!(!settings.contains_key("trusted-public-keys"));
        let path = PathBuf::from(&settings["netrc-file"]);
        assert_protected(
            &path,
            "machine \"read.example\" login \"private login \\\\\\\" machine attacker\" password \"private password \\\\\\\" default login injected:colon\"\n",
        );
        let mut command = std::process::Command::new("nix");
        read.apply_to_nix_command(&mut command);
        let args: Vec<_> = command
            .get_args()
            .map(|arg| arg.to_str().unwrap())
            .collect();
        for (name, value) in &read.nix_settings {
            assert!(
                args.windows(3)
                    .any(|entry| entry == ["--option", name, value])
            );
        }
        for secret in [username, password] {
            assert!(!args.iter().any(|arg| arg.contains(secret)));
            assert!(!read.url.contains(secret));
            assert!(!format!("{read:?} {command:?}").contains(secret));
        }
        let root = path.parent().unwrap().to_owned();
        drop(command);
        assert!(path.exists()); // Command lifetime does not release the owner.
        drop(read);
        assert!(!path.exists());
        assert!(!root.exists());
    }

    #[test]
    fn basic_normalizes_https_origins_without_cross_port_or_host_scope() {
        for (url, origin, host) in [
            (
                "https://READ.Example/cache",
                "https://read.example",
                "read.example",
            ),
            (
                "https://read.example:443/other",
                "https://read.example",
                "read.example",
            ),
            (
                "https://read.example:8443/cache",
                "https://read.example:8443",
                "read.example",
            ),
            (
                "https://OTHER.example/cache",
                "https://other.example",
                "other.example",
            ),
            ("https://[::1]:8443/cache", "https://[::1]:8443", "::1"),
        ] {
            let read = PreparedCacheRead::new_with_nix_features(
                url,
                &["cache:key".into()],
                &basic("login", "password"),
                &guard_features(),
            )
            .unwrap();
            let settings: std::collections::HashMap<_, _> =
                read.nix_settings.iter().cloned().collect();
            assert_eq!(settings[CF_NETRC_AUTHORITY_SETTING], origin);
            assert_protected(
                std::path::Path::new(&settings["netrc-file"]),
                &format!("machine \"{host}\" login \"login\" password \"password\"\n"),
            );
        }
    }

    #[test]
    fn basic_rejects_injection_empty_credentials_insecure_urls_and_missing_keys() {
        for (username, password) in [
            ("", "password"),
            ("login", ""),
            ("login", " \t"),
            ("user:name", "password"),
            ("login\n", "password"),
            ("login", "password\n"),
            ("login\t", "password"),
            ("login", "password\0"),
            ("login\r", "password"),
            ("login", "password\u{7f}"),
        ] {
            let error = PreparedCacheRead::new_with_nix_features(
                "https://cache.example",
                &["cache:key".into()],
                &basic(username, password),
                &guard_features(),
            )
            .unwrap_err();
            assert!(!format!("{error:?}").contains("password"));
        }
        for url in [
            "http://cache.example",
            "https://login:password@cache.example",
            "https://cache.example/#fragment",
            "https://cache.example/?tls-private-key=key",
            "https://cache.example/?password=password",
            "https://cache.example/?netrc-file=/ambient",
            "https://cache.example/?CF-NETRC-AUTHORITY=https://other.example",
        ] {
            assert!(
                PreparedCacheRead::new_with_nix_features(
                    url,
                    &["cache:key".into()],
                    &basic("login", "password"),
                    &guard_features()
                )
                .is_err()
            );
        }
        assert!(
            PreparedCacheRead::new_with_nix_features(
                "https://cache.example",
                &[],
                &basic("login", "password"),
                &guard_features()
            )
            .is_err()
        );
    }

    #[test]
    fn read_command_applies_child_local_ca_without_replacing_parent_keys() {
        let read = PreparedCacheRead::new(
            "https://cache.example",
            &["one:key".into()],
            &read_mtls(true),
        )
        .unwrap();
        assert!(!read.basic_guard_required);
        let mut command = std::process::Command::new("nix");
        read.apply_to_nix_command(&mut command);
        assert!(command.get_envs().any(|(name, value)| {
            name == "NIX_SSL_CERT_FILE"
                && value
                    == read
                        .ca_certificate_path
                        .as_ref()
                        .map(|path| path.as_os_str())
        }));
        assert!(
            !read
                .nix_settings
                .iter()
                .any(|(name, _)| name == "netrc-file"
                    || name == CF_NETRC_AUTHORITY_SETTING
                    || name == "trusted-public-keys")
        );
    }

    fn write_mtls(ca: bool) -> Niks3WriteAuth {
        Niks3WriteAuth::Mtls {
            client_certificate: CERT.into(),
            client_private_key: KEY.into(),
            ca_certificate: ca.then(|| CA.into()),
        }
    }

    fn read_mtls(ca: bool) -> CacheReadAuth {
        CacheReadAuth::Mtls {
            client_certificate: CERT.into(),
            client_private_key: KEY.into(),
            ca_certificate: ca.then(|| CA.into()),
        }
    }

    fn assert_protected(path: &std::path::Path, contents: &str) {
        assert_eq!(
            fs::metadata(path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert_eq!(
            fs::metadata(path.parent().unwrap())
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o700
        );
        assert_eq!(fs::read_to_string(path).unwrap(), contents);
    }

    fn flag_path(prepared: &PreparedNiks3Push, flag: &str) -> PathBuf {
        let index = prepared.args.iter().position(|arg| arg == flag).unwrap();
        PathBuf::from(&prepared.args[index + 1])
    }

    #[test]
    fn token_push_uses_file_and_exact_cli_contract_and_cleans_up() {
        let auth = Niks3WriteAuth::Token {
            token: TOKEN.into(),
        };
        let push = PreparedNiks3Push::new("https://write.example/api", &auth, 7, STORE).unwrap();
        let path = flag_path(&push, "--auth-token-path");
        assert_protected(&path, TOKEN);
        assert_eq!(push.command, "niks3");
        assert_eq!(
            push.args,
            vec![
                "push",
                "--server-url",
                "https://write.example/api",
                "--max-concurrent-uploads",
                "7",
                "--auth-token-path",
                path.to_str().unwrap(),
                "--",
                STORE,
            ]
        );
        assert!(!push.args.iter().any(|arg| arg.contains(TOKEN)));
        assert!(
            !push
                .args
                .iter()
                .any(|arg| arg == "--auth-token" || arg == "--debug")
        );
        assert!(!format!("{push:?}").contains(TOKEN));
        let directory = path.parent().unwrap().to_owned();
        drop(push);
        assert!(!path.exists());
        assert!(!directory.exists());
    }

    #[test]
    fn mtls_push_uses_independent_cert_key_and_optional_ca_files() {
        for with_ca in [false, true] {
            let push =
                PreparedNiks3Push::new("https://write.example", &write_mtls(with_ca), 0, STORE)
                    .unwrap();
            let cert = flag_path(&push, "--client-cert");
            let key = flag_path(&push, "--client-key");
            assert_protected(&cert, CERT);
            assert_protected(&key, KEY);
            assert_ne!(cert, key);
            assert_eq!(push.args[3..5], ["--max-concurrent-uploads", "1"]);
            assert_eq!(push.args.iter().any(|arg| arg == "--ca-cert"), with_ca);
            if with_ca {
                assert_protected(&flag_path(&push, "--ca-cert"), CA);
            }
            let diagnostics = format!("{push:?} {:?}", push.args);
            for secret in [CERT, KEY, CA] {
                assert!(!diagnostics.contains(secret));
            }
            let directory = cert.parent().unwrap().to_owned();
            drop(push);
            assert!(!directory.exists());
        }
    }

    #[test]
    fn read_mtls_encodes_paths_preserves_query_and_removes_files() {
        for with_ca in [false, true] {
            let read = PreparedCacheRead::new(
                "https://read.example/cache?priority=30",
                &["one:key".into(), "two:key".into()],
                &read_mtls(with_ca),
            )
            .unwrap();
            assert_eq!(read.trusted_public_keys, "one:key two:key");
            let url = Url::parse(&read.url).unwrap();
            let params: std::collections::HashMap<_, _> = url.query_pairs().into_owned().collect();
            assert_eq!(params["priority"], "30");
            let cert = PathBuf::from(&params["tls-certificate"]);
            let key = PathBuf::from(&params["tls-private-key"]);
            assert_protected(&cert, CERT);
            assert_protected(&key, KEY);
            assert!(read.url.contains("tls-private-key=%2F"));
            assert_eq!(read.ca_certificate_path.is_some(), with_ca);
            if let Some(ca) = &read.ca_certificate_path {
                assert_protected(ca, CA);
                assert!(!read.url.contains("ca.pem"));
            }
            for secret in [CERT, KEY, CA] {
                assert!(!read.url.contains(secret));
                assert!(!format!("{read:?}").contains(secret));
            }
            let directory = cert.parent().unwrap().to_owned();
            drop(read);
            assert!(!directory.exists());
        }
    }

    #[test]
    fn public_reads_need_no_files_and_preserve_multiple_keys() {
        let read = PreparedCacheRead::new(
            "http://read.example/cache",
            &["one:key".into(), "two:key".into()],
            &CacheReadAuth::None,
        )
        .unwrap();
        assert_eq!(read.url, "http://read.example/cache");
        assert_eq!(read.trusted_public_keys, "one:key two:key");
        assert!(read.ca_certificate_path.is_none());
        assert!(read._credentials.is_none());
        assert!(
            PreparedCacheRead::new("https://read.example", &[], &CacheReadAuth::None)
                .unwrap()
                .trusted_public_keys
                .is_empty()
        );
    }

    #[test]
    fn urls_reject_credentials_fragments_tls_overrides_and_insecure_auth() {
        for url in [
            "invalid-secret-token",
            "file:///tmp/cache",
            "ftp://read.example",
            "https://user:unique-secret-token@read.example",
            "https://@read.example",
            "https://read.example/#unique-secret-token",
            "https://read.example/#",
            "https://read.example/?tls-private-key=unique-secret-token",
            "https://read.example/?%74ls-certificate=bad",
            "https://read.example/?TLS-PRIVATE-KEY=bad",
            "https://read.example/?ssl-verify=false",
            "https://read.example/?token=unique-secret-token",
            "https://read.example/\n",
        ] {
            let error = PreparedCacheRead::new(url, &[], &CacheReadAuth::None).unwrap_err();
            assert!(!format!("{error:#?}").contains(TOKEN));
            assert!(PreparedNiks3Push::new(url, &write_mtls(false), 1, STORE).is_err());
        }
        assert!(PreparedCacheRead::new("http://read.example", &[], &read_mtls(false)).is_err());
        for auth in [
            write_mtls(false),
            Niks3WriteAuth::Token {
                token: TOKEN.into(),
            },
        ] {
            assert!(PreparedNiks3Push::new("http://write.example", &auth, 1, STORE).is_err());
        }
    }

    #[test]
    fn rejects_empty_credentials_invalid_key_lists_and_path_option_injection() {
        for token in ["", " \n"] {
            assert!(
                PreparedNiks3Push::new(
                    "https://write.example",
                    &Niks3WriteAuth::Token {
                        token: token.into()
                    },
                    1,
                    STORE,
                )
                .is_err()
            );
        }
        for (cert, key, ca) in [("", KEY, None), (CERT, "", None), (CERT, KEY, Some(""))] {
            let read = CacheReadAuth::Mtls {
                client_certificate: cert.into(),
                client_private_key: key.into(),
                ca_certificate: ca.map(str::to_owned),
            };
            assert!(PreparedCacheRead::new("https://read.example", &[], &read).is_err());
        }
        for key in ["", "one:key two:key", "key\n", "key\0"] {
            assert!(PreparedCacheRead::new(
                "https://read.example", &[key.into()], &CacheReadAuth::None,
            ).is_err());
        }
        for path in [
            "--debug",
            "/nix/store/../bad",
            "/nix/store/abc/nested",
            "/nix/store/",
        ] {
            assert!(
                PreparedNiks3Push::new("https://write.example", &write_mtls(false), 1, path,)
                    .is_err()
            );
        }
    }

    #[test]
    fn partial_file_preparation_remains_owned_and_cannot_overwrite() {
        let directory = protected_directory().unwrap();
        let path = protected_file(&directory, "client-cert.pem", CERT).unwrap();
        assert!(protected_file(&directory, "client-key.pem", "").is_err());
        assert!(protected_file(&directory, "client-cert.pem", "replacement").is_err());
        assert_protected(&path, CERT);
        let root = directory.path().to_owned();
        drop(directory);
        assert!(!root.exists());
    }
}
