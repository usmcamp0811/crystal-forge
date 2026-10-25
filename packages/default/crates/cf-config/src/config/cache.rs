use super::duration_serde;
pub use cf_protocol::cache::CacheType;
use cf_protocol::cache::{CacheReadAuth, Niks3WriteAuth};
use serde::Deserialize;
use std::time::Duration;

/// Configures publication and read-plane trust for one cache destination.
///
/// Niks3 write credentials and read credentials are independent. Consumers must
/// use the prepared credential helpers to own files until child processes exit.
#[derive(Clone, Debug, Deserialize)]
pub struct CacheConfig {
    /// Niks3 write API URL, distinct from the read/substituter URL.
    #[serde(default)]
    pub niks3_server_url: Option<String>,
    /// Authentication for Niks3 writes over HTTPS.
    #[serde(default)]
    pub niks3_write_auth: Option<Niks3WriteAuth>,
    /// Signing keys trusted for Niks3 cache reads.
    #[serde(default)]
    pub niks3_public_keys: Vec<String>,
    /// Independent read-plane authentication; absent means public reads.
    #[serde(default)]
    pub niks3_read_auth: CacheReadAuth,
    #[serde(default)]
    pub cache_type: CacheType,
    pub push_to: Option<String>,
    #[serde(default)]
    pub push_after_build: bool,
    pub signing_key: Option<String>,
    pub compression: Option<String>,
    pub push_filter: Option<Vec<String>>,
    #[serde(default = "CacheConfig::default_parallel_uploads")]
    pub parallel_uploads: u32,
    // S3-specific
    pub s3_region: Option<String>,
    pub s3_profile: Option<String>,
    pub s3_access_key_id: Option<String>,
    pub s3_secret_access_key: Option<String>,
    pub s3_session_token: Option<String>,
    pub s3_endpoint_url: Option<String>,
    // Attic-specific
    pub attic_token: Option<String>,
    pub attic_cache_name: Option<String>,
    pub attic_public_key: Option<String>,
    pub attic_ignore_upstream_cache_filter: bool,
    pub attic_jobs: u32,
    // Retry configuration
    #[serde(default)]
    pub max_retries: u32,
    #[serde(default)]
    pub retry_delay_seconds: u64,
    #[serde(
        default = "CacheConfig::default_poll_interval",
        with = "duration_serde"
    )]
    pub poll_interval: Duration,
    /// Timeout for each cache push attempt in seconds (default: 3600 = 1 hour)
    #[serde(default = "CacheConfig::default_push_timeout_seconds")]
    pub push_timeout_seconds: u64,
    #[serde(default)]
    pub force_repush: bool,
    pub require_sigs: bool,
}

#[derive(Debug, Clone)]
pub struct CacheCommand {
    pub command: String,
    pub args: Vec<String>,
}

impl CacheConfig {
    fn default_parallel_uploads() -> u32 {
        1
    }

    fn default_poll_interval() -> Duration {
        Duration::from_secs(30)
    }

    pub fn default_push_timeout_seconds() -> u64 {
        3600 // 1 hour
    }

    /// Signs the source closure before transfer when a signing key is configured.
    ///
    /// Source signing alone does not update already-valid destination paths.
    /// Signed Nix publication must also run [`Self::signature_commands`] after
    /// [`Self::cache_command`] succeeds.
    pub fn sign_command(&self, store_path: &str) -> Option<CacheCommand> {
        let key_path = self.signing_key.as_ref()?;
        Some(CacheCommand {
            command: "nix".to_string(),
            args: vec![
                "store".to_string(),
                "sign".to_string(),
                "--recursive".to_string(),
                "--key-file".to_string(),
                key_path.clone(),
                store_path.to_string(),
            ],
        })
    }

    /// Returns the command and arguments for cache operations.
    ///
    /// Returns `None` for Niks3: a generic command cannot own authentication
    /// files. Use [`crate::cache_credentials::PreparedNiks3Push`] instead.
    pub fn cache_command(&self, store_path: &str) -> Option<CacheCommand> {
        match self.cache_type {
            CacheType::Niks3 => None,
            CacheType::S3 => self.s3_cache_command(store_path),
            CacheType::Attic => self.attic_cache_command(store_path),
            CacheType::Http | CacheType::Nix => self.nix_cache_command(store_path),
        }
    }

    /// Returns the ordered metadata synchronization and destination trust checks.
    ///
    /// Signed `Nix` publication completes only after transfer and these commands
    /// succeed. `source_store` must identify the store used for source signing
    /// and transfer (normally `NIX_REMOTE`, or `auto` when unset). The destination
    /// uses `addSignatures` for synchronization, as supported by `ssh-ng://` in
    /// Nix 2.34.8. Legacy `ssh://` instead runs verification only: complete CF
    /// trust succeeds without attempting unsupported writes of unrelated source
    /// signatures. Missing CF trust fails verification and cannot be repaired
    /// through that protocol. Other adapters keep their native contract.
    ///
    /// `copy-sigs` checks NAR hash, size and references before adding signatures,
    /// but silently skips mismatches. The verification command checks destination
    /// metadata recursively with only the configured key's public half. It does
    /// not fetch source signatures during verification or accept `ultimate` as
    /// a substitute for trust. Nix also accepts verifiable content-addressed paths.
    /// No private key is sent to the destination. Failure can leave transferred
    /// paths or partial metadata updates; retrying these steps is idempotent.
    /// The read endpoint must expose the same store metadata, and target readers
    /// must trust the configured key's public half and enforce signature checks.
    ///
    /// Success establishes closure metadata trust, not target credentials,
    /// network access, retention against GC, or a content-integrity scan.
    ///
    /// # Examples
    /// ```
    /// use cf_config::CacheConfig;
    /// let cache = CacheConfig {
    ///     push_to: Some("ssh-ng://cache@host".into()),
    ///     signing_key: Some("/run/keys/cache".into()),
    ///     ..Default::default()
    /// };
    /// assert_eq!(cache.signature_commands("/nix/store/output", "daemon").len(), 2);
    /// ```
    pub fn signature_commands(&self, store_path: &str, source_store: &str) -> Vec<CacheCommand> {
        let (CacheType::Nix, Some(destination), Some(key)) =
            (&self.cache_type, &self.push_to, &self.signing_key)
        else {
            return Vec::new();
        };
        let mut commands = vec![
            CacheCommand {
                command: "nix".into(),
                args: [
                    "store",
                    "copy-sigs",
                    "--store",
                    destination,
                    "--substituter",
                    source_store,
                    "--recursive",
                    store_path,
                ]
                .into_iter()
                .map(String::from)
                .collect(),
            },
            CacheCommand {
                command: "nix".into(),
                args: [
                    "store",
                    "verify",
                    "--store",
                    destination,
                    "--recursive",
                    "--no-contents",
                    "--sigs-needed",
                    "1",
                    "--option",
                    "trusted-public-keys",
                    "",
                    "--option",
                    "extra-trusted-public-keys",
                    "",
                    "--option",
                    "secret-key-files",
                    key,
                    store_path,
                ]
                .into_iter()
                .map(String::from)
                .collect(),
            },
        ];
        if destination.starts_with("ssh://") {
            commands.remove(0);
        }
        commands
    }

    /// Legacy: still returns args only.
    pub fn copy_command_args(&self, store_path: &str) -> Option<Vec<String>> {
        self.cache_command(store_path).map(|cmd| cmd.args)
    }

    fn attic_cache_command(&self, store_path: &str) -> Option<CacheCommand> {
        let cache_name = self.attic_cache_name.as_ref()?;
        let mut args = vec![
            "push".to_string(),
            cache_name.clone(),
            store_path.to_string(),
        ];
        if self.attic_ignore_upstream_cache_filter {
            args.push("--ignore-upstream-cache-filter".to_string());
        }
        args.extend(["--jobs".to_string(), self.attic_jobs.to_string()]);
        Some(CacheCommand {
            command: "attic".to_string(),
            args,
        })
    }

    fn s3_cache_command(&self, store_path: &str) -> Option<CacheCommand> {
        let push_to = self.push_to.as_ref()?;
        let mut args = vec!["copy".to_string(), "--to".to_string(), push_to.clone()];
        if self.force_repush {
            args.push("--refresh".to_string());
        }
        if let Some(compression) = &self.compression {
            args.extend(["--compression".to_string(), compression.clone()]);
        }
        args.push(store_path.to_string());
        Some(CacheCommand {
            command: "nix".to_string(),
            args,
        })
    }

    fn nix_cache_command(&self, store_path: &str) -> Option<CacheCommand> {
        let push_to = self.push_to.as_ref()?;
        let mut args = vec!["copy".to_string(), "--to".to_string(), push_to.clone()];
        if self.force_repush {
            args.push("--refresh".to_string());
        }
        if let Some(compression) = &self.compression {
            args.extend(["--compression".to_string(), compression.clone()]);
        }
        args.push(store_path.to_string());
        Some(CacheCommand {
            command: "nix".to_string(),
            args,
        })
    }

    pub fn should_push(&self, target_name: &str) -> bool {
        if !self.push_after_build {
            return false;
        }
        match &self.push_filter {
            Some(filters) => filters.iter().any(|filter| target_name.contains(filter)),
            None => true,
        }
    }
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            niks3_server_url: None,
            niks3_write_auth: None,
            niks3_public_keys: Vec::new(),
            niks3_read_auth: CacheReadAuth::None,
            cache_type: CacheType::Nix,
            push_to: None,
            push_after_build: false,
            signing_key: None,
            compression: None,
            push_filter: None,
            parallel_uploads: Self::default_parallel_uploads(),
            s3_region: None,
            s3_profile: None,
            s3_access_key_id: None,
            s3_secret_access_key: None,
            s3_session_token: None,
            s3_endpoint_url: None,
            attic_token: None,
            attic_cache_name: None,
            attic_public_key: None,
            attic_ignore_upstream_cache_filter: true,
            attic_jobs: 5,
            max_retries: 3,
            retry_delay_seconds: 5,
            poll_interval: Self::default_poll_interval(),
            push_timeout_seconds: Self::default_push_timeout_seconds(),
            force_repush: false,
            require_sigs: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signature_publication_is_limited_to_signed_nix_destinations() {
        let mut cache = CacheConfig {
            push_to: Some("ssh-ng://cache@host".into()),
            signing_key: Some("/run/keys/cf".into()),
            ..Default::default()
        };
        let commands = cache.signature_commands("/nix/store/output", "daemon");
        assert_eq!(commands.len(), 2);
        assert_eq!(
            commands[0].args,
            [
                "store",
                "copy-sigs",
                "--store",
                "ssh-ng://cache@host",
                "--substituter",
                "daemon",
                "--recursive",
                "/nix/store/output"
            ]
        );
        cache.push_to = Some("ssh://cache@host".into());
        let legacy = cache.signature_commands("/nix/store/output", "daemon");
        assert_eq!(legacy.len(), 1);
        assert_eq!(legacy[0].args[1], "verify");
        assert!(!legacy[0].args.iter().any(|arg| arg == "--substituter"));
        cache.push_to = Some("ssh-ng://cache@host".into());
        for adapter in [
            CacheType::S3,
            CacheType::Http,
            CacheType::Attic,
            CacheType::Niks3,
        ] {
            cache.cache_type = adapter;
            assert!(
                cache
                    .signature_commands("/nix/store/output", "daemon")
                    .is_empty()
            );
        }
        cache.cache_type = CacheType::Nix;
        cache.signing_key = None;
        assert!(
            cache
                .signature_commands("/nix/store/output", "daemon")
                .is_empty()
        );
    }

    // Run explicitly in the prepared Nix environment. Cargo-only builds do not
    // supply Nix. This fixture owns local Nix databases, never the host daemon.
    #[test]
    #[ignore = "requires prepared Nix 2.34.8; uses only isolated local stores"]
    fn warm_nix_publication_propagates_closure_trust() {
        use std::io::Write;
        use std::process::{Command, Output, Stdio};

        let nix = std::env::var("NIX_TEST_BIN").unwrap_or_else(|_| "nix".into());
        let nix_store = std::path::Path::new(&nix).with_file_name("nix-store");
        let parent = std::env::temp_dir();
        assert!(parent.is_dir());
        let fixture = tempfile::Builder::new()
            .prefix("cf-publication-")
            .tempdir_in(parent)
            .unwrap();
        let source_root = fixture.path().join("source");
        let destination_root = fixture.path().join("destination");
        let consumer_root = fixture.path().join("consumer");
        // Nix builds can inherit NIX_STORE_DIR=/dummy. Root relocates physical
        // files; explicitly keep virtual paths at /nix/store for this fixture.
        let store =
            |root: &std::path::Path| format!("local?root={}&store=/nix/store", root.display());
        let source = store(&source_root);
        let destination = store(&destination_root);
        let consumer = store(&consumer_root);
        let run = |args: &[&str]| -> Output {
            Command::new(&nix)
                .env("NIX_CONFIG", "experimental-features = nix-command\nsubstituters =\nbuilders =\nrequire-sigs = true\n")
                .env_remove("NIX_REMOTE")
                .env("NIX_USER_CONF_FILES", "/dev/null")
                .env("HOME", fixture.path())
                .env("XDG_CACHE_HOME", fixture.path().join("cache"))
                .args(args).output().unwrap()
        };
        let success = |output: Output| -> String {
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            String::from_utf8(output.stdout).unwrap()
        };
        // Input-addressed fixtures are registered only in the source store.
        // A CA fixture would bypass signature checking and hide this regression.
        let dependency = "/nix/store/00000000000000000000000000000000-dependency";
        let output = "/nix/store/11111111111111111111111111111111-output";
        success(run(&["store", "info", "--store", &source]));
        let physical_store = source_root.join("nix/store");
        std::fs::write(
            physical_store.join(dependency.rsplit('/').next().unwrap()),
            "dependency\n",
        )
        .unwrap();
        std::fs::write(
            physical_store.join(output.rsplit('/').next().unwrap()),
            dependency,
        )
        .unwrap();
        let mut register = Command::new(&nix_store)
            .args(["--store", &source, "--register-validity"])
            .env("NIX_CONFIG", "substituters =\nbuilders =\n")
            .env_remove("NIX_REMOTE")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        write!(
            register.stdin.take().unwrap(),
            "{dependency}\n\n0\n{output}\n\n1\n{dependency}\n"
        )
        .unwrap();
        success(register.wait_with_output().unwrap());
        let key = fixture.path().join("cf.key");
        let secret = success(run(&[
            "key",
            "generate-secret",
            "--key-name",
            "cf-publication-test",
        ]));
        std::fs::write(&key, &secret).unwrap();
        // Public key extraction uses the same Nix implementation as production.
        let mut convert = Command::new(&nix)
            .args(["key", "convert-secret-to-public"])
            .env("NIX_CONFIG", "experimental-features = nix-command\n")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        convert
            .stdin
            .take()
            .unwrap()
            .write_all(secret.as_bytes())
            .unwrap();
        let public = success(convert.wait_with_output().unwrap());
        let cache = CacheConfig {
            push_to: Some(destination.clone()),
            signing_key: Some(key.to_str().unwrap().into()),
            ..Default::default()
        };
        let execute = |operation: CacheCommand| {
            let args = operation
                .args
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>();
            run(&args)
        };
        // Warm the destination with a different key, never trusted by consumers.
        let other_key = fixture.path().join("other.key");
        std::fs::write(
            &other_key,
            success(run(&["key", "generate-secret", "--key-name", "other"])),
        )
        .unwrap();
        success(run(&[
            "store",
            "sign",
            "--store",
            &source,
            "--key-file",
            other_key.to_str().unwrap(),
            dependency,
        ]));
        success(run(&[
            "copy",
            "--from",
            &source,
            "--to",
            &destination,
            "--option",
            "secret-key-files",
            other_key.to_str().unwrap(),
            dependency,
        ]));
        // Reproduce a prior build signed only with a different static key.
        // The selected publication key must cover the entire resolved root.
        success(run(&[
            "store",
            "sign",
            "--recursive",
            "--store",
            &source,
            "--key-file",
            other_key.to_str().unwrap(),
            output,
        ]));
        let mut sign = cache.sign_command(output).unwrap();
        sign.args.extend(["--store".into(), source.clone()]);
        success(execute(sign));
        let metadata = |store: &str| -> serde_json::Value {
            serde_json::from_str(&success(run(&[
                "path-info",
                "--store",
                store,
                "--json",
                "--recursive",
                output,
            ])))
            .unwrap()
        };
        let source_metadata = metadata(&source);
        for path in [dependency, output] {
            assert!(
                source_metadata[path]["signatures"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|s| s.as_str().unwrap().starts_with("cf-publication-test:"))
            );
            assert!(source_metadata[path]["ca"].is_null());
        }
        let mut transfer = cache.cache_command(output).unwrap();
        transfer.args.extend([
            "--from".into(),
            source.clone(),
            "--option".into(),
            "secret-key-files".into(),
            key.to_str().unwrap().into(),
        ]);
        success(execute(transfer));
        let before = metadata(&destination);
        assert!(
            before[output]["signatures"]
                .as_array()
                .unwrap()
                .iter()
                .any(|s| s.as_str().unwrap().starts_with("cf-publication-test:"))
        );
        assert!(
            !before[dependency]["signatures"]
                .as_array()
                .unwrap()
                .iter()
                .any(|s| s.as_str().unwrap().starts_with("cf-publication-test:"))
        );
        let acquire = || {
            run(&[
                "copy",
                "--from",
                &destination,
                "--to",
                &consumer,
                "--option",
                "trusted-public-keys",
                public.trim(),
                "--option",
                "extra-trusted-public-keys",
                "",
                "--option",
                "secret-key-files",
                "",
                output,
            ])
        };
        let rejected = acquire();
        assert!(!rejected.status.success());
        assert!(String::from_utf8_lossy(&rejected.stderr).contains("signature"));
        assert!(
            !run(&["path-info", "--store", &consumer, dependency])
                .status
                .success()
        );
        let commands = cache.signature_commands(output, &source);
        let legacy = CacheConfig {
            push_to: Some("ssh://fixture".into()),
            ..cache.clone()
        };
        let legacy_verify = || {
            let mut commands = legacy.signature_commands(output, &source);
            assert_eq!(commands.len(), 1);
            assert_eq!(commands[0].args[1], "verify");
            // Exercise the legacy command policy with real local metadata;
            // SSH topology belongs to the owning Delivery integration scenario.
            commands[0].args[3] = destination.clone();
            commands.remove(0)
        };
        let legacy_untrusted = execute(legacy_verify());
        assert!(!legacy_untrusted.status.success());
        // Verification alone must fail despite successful source signing/copy.
        let untrusted = execute(commands[1].clone());
        assert!(!untrusted.status.success());
        for operation in commands {
            success(execute(operation));
        }
        let after = metadata(&destination);
        for path in [dependency, output] {
            assert!(
                after[path]["signatures"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|s| s.as_str().unwrap().starts_with("cf-publication-test:"))
            );
        }
        assert!(
            after[dependency]["signatures"]
                .as_array()
                .unwrap()
                .iter()
                .any(|s| s.as_str().unwrap().starts_with("other:"))
        );
        // A retry neither requires retransferring NARs nor removes other keys.
        for operation in cache.signature_commands(output, &source) {
            success(execute(operation));
        }
        assert_eq!(after, metadata(&destination));
        // Add a source signature after transfer. Legacy publication must not
        // attempt unsupported addSignatures merely to copy this unrelated key.
        let late_key = fixture.path().join("late.key");
        std::fs::write(
            &late_key,
            success(run(&["key", "generate-secret", "--key-name", "late"])),
        )
        .unwrap();
        success(run(&[
            "store",
            "sign",
            "--recursive",
            "--store",
            &source,
            "--key-file",
            late_key.to_str().unwrap(),
            output,
        ]));
        assert!(
            metadata(&source)[dependency]["signatures"]
                .as_array()
                .unwrap()
                .iter()
                .any(|s| s.as_str().unwrap().starts_with("late:"))
        );
        assert!(
            !metadata(&destination)[dependency]["signatures"]
                .as_array()
                .unwrap()
                .iter()
                .any(|s| s.as_str().unwrap().starts_with("late:"))
        );
        success(execute(legacy_verify()));
        eprintln!(
            "legacy policy: verify-only; missing CF trust={}; complete CF trust with unrelated source-only signature=0",
            legacy_untrusted.status
        );
        success(acquire());
        let acquired = metadata(&consumer);
        assert!(acquired.get(dependency).is_some());
        assert!(acquired.get(output).is_some());
        eprintln!(
            "warm publication: source CF signatures=2/2; destination CF signatures before=1/2, after=2/2; pre-propagation verify={}; pre-propagation acquisition={}; post-propagation acquisition=0; repeated propagation idempotent",
            untrusted.status, rejected.status
        );
    }

    #[test]
    fn legacy_cache_configuration_defaults_niks3_fields() {
        let config: CacheConfig = serde_json::from_value(serde_json::json!({
            "attic_ignore_upstream_cache_filter":true, "attic_jobs":5, "require_sigs":true
        }))
        .unwrap();
        assert!(config.niks3_server_url.is_none());
        assert!(config.niks3_write_auth.is_none());
        assert!(config.niks3_public_keys.is_empty());
        assert_eq!(config.niks3_read_auth, CacheReadAuth::None);
        let default = CacheConfig::default();
        assert!(default.niks3_server_url.is_none());
        assert!(default.niks3_write_auth.is_none());
        assert!(default.niks3_public_keys.is_empty());
        assert_eq!(default.niks3_read_auth, CacheReadAuth::None);
    }

    #[test]
    fn niks3_config_requires_prepared_command_and_redacts_auth() {
        let config: CacheConfig = serde_json::from_value(serde_json::json!({
            "cache_type":"Niks3", "push_to":"https://read.example",
            "niks3_server_url":"https://write.example",
            "niks3_write_auth":{"kind":"token", "token":"secret-token"},
            "niks3_public_keys":["one:key", "two:key"],
            "niks3_read_auth":{"kind":"mtls", "client_certificate":"secret-cert",
                "client_private_key":"secret-key", "ca_certificate":"secret-ca"},
            "attic_ignore_upstream_cache_filter":true, "attic_jobs":5, "require_sigs":true
        }))
        .unwrap();
        assert_eq!(config.cache_type, CacheType::Niks3);
        assert_eq!(config.niks3_public_keys, ["one:key", "two:key"]);
        assert!(config.cache_command("/nix/store/abc-output").is_none());
        assert!(config.copy_command_args("/nix/store/abc-output").is_none());
        for secret in ["secret-token", "secret-key", "secret-cert", "secret-ca"] {
            assert!(!format!("{config:?}").contains(secret));
        }
    }
}
