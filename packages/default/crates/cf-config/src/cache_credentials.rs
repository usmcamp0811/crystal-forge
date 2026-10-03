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

/// Owns read-plane Nix settings and optional protected TLS credential files.
///
/// The URL references files using encoded Nix store parameters. It contains no
/// private-key contents. Consumers must keep this owner alive until Nix exits
/// and preserve signature verification when applying the public keys.
pub struct PreparedCacheRead {
    /// Substituter URL with generated mTLS file parameters when required.
    pub url: String,
    /// Space-separated keys for Nix's `trusted-public-keys` option.
    pub trusted_public_keys: String,
    /// Optional trust bundle to apply through child-local `NIX_SSL_CERT_FILE`.
    /// Consumers must not replace the parent process's environment globally.
    pub ca_certificate_path: Option<PathBuf>,
    _credentials: Option<TempDir>,
}

impl std::fmt::Debug for PreparedCacheRead {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("PreparedCacheRead([REDACTED])")
    }
}

impl PreparedCacheRead {
    /// Prepares public HTTP(S) reads or authenticated HTTPS reads.
    ///
    /// Existing non-credential query parameters are preserved. Keys are joined
    /// in input order; empty or whitespace-containing key entries are rejected
    /// to prevent accidental changes to Nix's space-separated trust list.
    ///
    /// # Errors
    /// Returns a credential-free error for invalid URLs, insecure authenticated
    /// transport, invalid key entries, empty credentials, or temporary-file I/O.
    /// PEM and signing-key cryptographic validity are checked by Nix.
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
        let mut url = validated_url(url, !matches!(auth, CacheReadAuth::None))?;
        ensure!(
            keys.iter()
                .all(|key| !key.is_empty()
                    && !key.chars().any(|c| c.is_whitespace() || c.is_control())),
            "invalid cache public key entry"
        );
        let mut ca_certificate_path = None;
        let credentials = match auth {
            CacheReadAuth::None => None,
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
            _credentials: credentials,
        })
    }
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
