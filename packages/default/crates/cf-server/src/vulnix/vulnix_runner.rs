use crate::config::VulnixConfig;
use crate::vulnix::process_group::{ScannerProcessGroup, isolate};
use crate::vulnix::vulnix_parser::VulnixEntry;

use anyhow::{Context, Result, anyhow};
use sqlx::PgPool;
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::io::AsyncReadExt;
use tokio::process::Command as AsyncCommand;
use tracing::{error, info};

/// Array of VulnixEntry - this is what vulnix outputs as JSON
pub type VulnixScanOutput = Vec<VulnixEntry>;

const SCANNER_STDOUT_LIMIT: usize = 8 * 1024 * 1024;
const SCANNER_STDERR_LIMIT: usize = 64 * 1024;
const OUTPUT_DRAIN_GRACE: Duration = Duration::from_secs(1);

/// Contains parsed evidence and bounded process diagnostics for one execution.
#[derive(Debug)]
pub struct VulnixScanExecution {
    /// Parsed vulnerability evidence.
    pub entries: VulnixScanOutput,
    /// Raw bounded stderr. Callers must redact it before persistence or logging.
    pub stderr: String,
    /// Is `true` when stderr exceeded the retained byte bound.
    pub stderr_truncated: bool,
    /// Scanner process exit code, or `None` when terminated by a signal.
    pub exit_code: Option<i32>,
}

/// Describes a failed local scanner execution and its bounded stderr.
#[derive(Debug)]
pub struct VulnixScanExecutionError {
    error: anyhow::Error,
    /// Raw bounded stderr. Callers must redact it before persistence or logging.
    pub stderr: String,
    /// Is `true` when stderr exceeded the retained byte bound.
    pub stderr_truncated: bool,
}

impl VulnixScanExecutionError {
    fn new(error: anyhow::Error) -> Self {
        Self {
            error,
            stderr: String::new(),
            stderr_truncated: false,
        }
    }

    fn with_stderr(error: anyhow::Error, stderr: &[u8], stderr_truncated: bool) -> Self {
        Self {
            error,
            stderr: String::from_utf8_lossy(stderr).into_owned(),
            stderr_truncated,
        }
    }

    #[cfg(test)]
    pub(crate) fn test_fixture(message: &str, stderr: &str, stderr_truncated: bool) -> Self {
        Self::with_stderr(
            anyhow!(message.to_string()),
            stderr.as_bytes(),
            stderr_truncated,
        )
    }
}

impl std::fmt::Display for VulnixScanExecutionError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.error.fmt(formatter)
    }
}

impl std::error::Error for VulnixScanExecutionError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.error.source()
    }
}

impl From<anyhow::Error> for VulnixScanExecutionError {
    fn from(error: anyhow::Error) -> Self {
        Self::new(error)
    }
}

/// Interprets a finished vulnix process.
///
/// vulnix overloads exit code 2. Its `output()` returns 2 when the JSON report
/// contains at least one unwhitelisted vulnerability, while `main()` maps every
/// uncaught `RuntimeError` to `sys.exit(2)`; `DeriverLookupError` is such a
/// `RuntimeError`. Click additionally uses exit code 2 for usage errors. The
/// exit status alone therefore cannot distinguish a completed scan that found
/// vulnerabilities from a fatal failure, so a result is accepted only when
/// stdout parses as the expected JSON report.
///
/// A fatal vulnix run writes its diagnostic to stderr and leaves stdout empty.
/// Reporting that case as a JSON parse error discards the only description of
/// the real cause, so the stderr text is preserved instead.
///
/// # Errors
///
/// Returns an error when the process reported a status other than 0 or 2, and
/// when a status of 2 is not accompanied by a parseable JSON report. Both error
/// paths include vulnix stderr.
fn parse_successful_vulnix_output(
    exit_code: Option<i32>,
    stdout: &str,
    stderr: &str,
) -> Result<VulnixScanOutput> {
    if !matches!(exit_code, Some(0 | 2)) {
        return Err(vulnix_process_failure(exit_code, stderr));
    }

    match serde_json::from_str(stdout) {
        Ok(entries) => Ok(entries),
        // COMPATIBILITY: exit 2 without a parseable report is a fatal vulnix
        // error, not malformed success output. Surfacing stderr keeps the
        // deriver-lookup and usage diagnostics that identify the real cause.
        Err(_) if exit_code == Some(2) => Err(vulnix_process_failure(exit_code, stderr)),
        Err(error) => {
            let stderr = stderr.trim();
            let stderr = if stderr.is_empty() {
                "vulnix produced no stderr output"
            } else {
                stderr
            };
            Err(anyhow!(
                "Failed to parse vulnix JSON output: {error}; stderr: {stderr}"
            ))
        }
    }
}

/// Builds the vulnix process-failure error, always including stderr.
///
/// Callers rely on this text to diagnose deployment problems, so an empty
/// stderr is reported explicitly instead of producing an error with no cause.
fn vulnix_process_failure(exit_code: Option<i32>, stderr: &str) -> anyhow::Error {
    let stderr = stderr.trim();
    let stderr = if stderr.is_empty() {
        "vulnix produced no stderr output"
    } else {
        stderr
    };
    anyhow!(
        "Vulnix scan process failed with exit code {}: {}",
        exit_code
            .map(|code| code.to_string())
            .unwrap_or_else(|| "terminated by signal".to_string()),
        stderr
    )
}

#[derive(Debug)]
pub struct VulnixRunner {
    config: VulnixConfig,
}

impl VulnixRunner {
    pub fn new() -> Self {
        Self {
            config: VulnixConfig::default(),
        }
    }

    pub fn with_config(config: &VulnixConfig) -> Self {
        Self {
            config: config.clone(),
        }
    }

    /// Check if vulnix is available on the system
    pub async fn check_vulnix_available() -> bool {
        Self::get_vulnix_version().await.is_ok()
    }

    /// Get vulnix version string
    pub async fn get_vulnix_version() -> Result<String> {
        let mut command = AsyncCommand::new("vulnix");
        command.arg("--version");
        let output = run_scanner_command(command, Duration::from_secs(5)).await?;

        if output.status.success() {
            let version = String::from_utf8_lossy(&output.stdout).trim().to_string();
            Ok(version)
        } else {
            Err(anyhow!("Failed to get vulnix version"))
        }
    }

    /// Scan a specific derivation
    pub async fn scan_derivation(
        &self,
        pool: &PgPool,
        derivation_id: i32,
        vulnix_version: Option<String>,
    ) -> Result<VulnixScanOutput> {
        Ok(self
            .scan_derivation_with_diagnostics(pool, derivation_id, vulnix_version)
            .await
            .map_err(anyhow::Error::new)?
            .entries)
    }

    /// Scans a derivation and returns bounded stderr with parsed evidence.
    ///
    /// # Errors
    ///
    /// Returns an error when scanner identity, input availability, execution,
    /// timeout, or JSON parsing fails.
    pub async fn scan_derivation_with_diagnostics(
        &self,
        pool: &PgPool,
        derivation_id: i32,
        vulnix_version: Option<String>,
    ) -> std::result::Result<VulnixScanExecution, VulnixScanExecutionError> {
        let expected_version = vulnix_version.context("CVE scan has no probed vulnix identity")?;
        let actual_version = Self::get_vulnix_version()
            .await
            .context("Failed to verify vulnix identity before execution")?;
        if actual_version != expected_version {
            return Err(anyhow!(
                "Vulnix identity changed after scheduling: expected {:?}, found {:?}",
                expected_version,
                actual_version
            )
            .into());
        }

        // Fetch store path in a separate scope so connection is released
        let store_path = {
            let derivation =
                crate::queries::derivations::get_derivation_by_id(pool, derivation_id).await?;
            derivation
                .store_path
                .ok_or_else(|| anyhow!("Derivation {} has no store_path", derivation_id))?
        }; // Connection released here when `derivation` goes out of scope

        // Only scan if the path exists
        if !tokio::fs::try_exists(&store_path).await.unwrap_or(false) {
            return Err(anyhow!("Derivation store_path does not exist: {}", store_path).into());
        }

        info!(
            "🔍 Scanning derivation {} with store path: {}",
            derivation_id, store_path
        );

        // Build vulnix command
        let mut cmd = AsyncCommand::new("vulnix");
        cmd.arg("--json").arg(&store_path);

        if self.config.enable_whitelist {
            cmd.arg("--whitelist").arg("/etc/vulnix-whitelist.toml");
        }

        // Add extra args
        for arg in &self.config.extra_args {
            cmd.arg(arg);
        }

        // Log the exact command being executed
        let program = cmd.as_std().get_program();
        let args: Vec<&std::ffi::OsStr> = cmd.as_std().get_args().collect();
        let args_str: Vec<String> = args
            .iter()
            .map(|arg| arg.to_string_lossy().to_string())
            .collect();
        info!("🔧 Executing command: {:?} {}", program, args_str.join(" "));

        match run_scanner_command(cmd, self.config.timeout).await {
            Ok(output) => {
                let stdout_msg = String::from_utf8_lossy(&output.stdout);
                let stderr_msg = String::from_utf8_lossy(&output.stderr);

                info!("🔍 Vulnix exit code: {}", output.status);
                info!("🔍 Stdout length: {} bytes", output.stdout.len());
                info!("🔍 Stderr length: {} bytes", output.stderr.len());

                if matches!(output.status.code(), Some(0 | 2)) {
                    let vulnix_entries = parse_successful_vulnix_output(
                        output.status.code(),
                        &stdout_msg,
                        &stderr_msg,
                    )
                    .map_err(|error| {
                        VulnixScanExecutionError::with_stderr(
                            error,
                            &output.stderr,
                            output.stderr_truncated,
                        )
                    })?;
                    info!(
                        "✅ Vulnix scan completed successfully with {} entries",
                        vulnix_entries.len()
                    );
                    Ok(VulnixScanExecution {
                        entries: vulnix_entries,
                        stderr: stderr_msg.into_owned(),
                        stderr_truncated: output.stderr_truncated,
                        exit_code: output.status.code(),
                    })
                } else {
                    error!("❌ Vulnix scan failed with exit code: {}", output.status);
                    error!(
                        "❌ stderr: {}",
                        crate::security::snapshot_redaction::redact_text(&stderr_msg)
                    );
                    parse_successful_vulnix_output(output.status.code(), &stdout_msg, &stderr_msg)
                        .map(|entries| VulnixScanExecution {
                            entries,
                            stderr: stderr_msg.into_owned(),
                            stderr_truncated: output.stderr_truncated,
                            exit_code: output.status.code(),
                        })
                        .map_err(|error| {
                            VulnixScanExecutionError::with_stderr(
                                error,
                                &output.stderr,
                                output.stderr_truncated,
                            )
                        })
                }
            }
            Err(error) => {
                error!(
                    "❌ Failed to execute vulnix command: {}",
                    crate::security::snapshot_redaction::redact_text(&error.to_string())
                );
                Err(error)
            }
        }
    }

    /// Backward compatibility method - delegates to scan_derivation
    pub async fn scan_target(
        &self,
        pool: &PgPool,
        derivation_id: i32,
        vulnix_version: Option<String>,
    ) -> Result<VulnixScanOutput> {
        self.scan_derivation(pool, derivation_id, vulnix_version)
            .await
    }
}

/// Runs one vulnix command in an isolated process group.
///
/// Timeout and future cancellation terminate the complete group. The function
/// Concurrent readers drain stdout and stderr while the child runs. A timeout
/// kills and reaps the process group, then gives the readers one bounded grace
/// period to publish bytes already emitted before it snapshots stderr.
#[derive(Debug)]
struct ScannerCommandOutput {
    status: std::process::ExitStatus,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
    stderr_truncated: bool,
}

#[derive(Debug, Default)]
struct BoundedOutput {
    bytes: Vec<u8>,
    truncated: bool,
}

type SharedBoundedOutput = Arc<Mutex<BoundedOutput>>;

fn bounded_output_snapshot(output: &SharedBoundedOutput) -> (Vec<u8>, bool) {
    let output = output
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    (output.bytes.clone(), output.truncated)
}

async fn finish_output_reader(
    task: &mut tokio::task::JoinHandle<std::io::Result<()>>,
    output: &SharedBoundedOutput,
    stream: &str,
) -> Result<(Vec<u8>, bool)> {
    task.await
        .with_context(|| format!("vulnix {stream} reader task failed"))??;
    Ok(bounded_output_snapshot(output))
}

async fn capture_terminated_output(
    task: &mut tokio::task::JoinHandle<std::io::Result<()>>,
    output: &SharedBoundedOutput,
) -> (Vec<u8>, bool) {
    if tokio::time::timeout(OUTPUT_DRAIN_GRACE, &mut *task)
        .await
        .is_err()
    {
        task.abort();
    }
    bounded_output_snapshot(output)
}

fn timeout_error(timeout: Duration, stderr_truncated: bool) -> anyhow::Error {
    let truncation = if stderr_truncated {
        " (stderr capture truncated)"
    } else {
        ""
    };
    anyhow!(
        "vulnix timed out after {} seconds{truncation}",
        timeout.as_secs()
    )
}

async fn run_scanner_command(
    mut command: AsyncCommand,
    timeout: Duration,
) -> std::result::Result<ScannerCommandOutput, VulnixScanExecutionError> {
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    isolate(&mut command);
    let child = command.spawn().context("Failed to spawn vulnix")?;
    let mut group = ScannerProcessGroup::new(child, "vulnix")?;
    let stdout = group
        .child_mut()
        .stdout
        .take()
        .context("vulnix stdout was not piped")?;
    let stderr = group
        .child_mut()
        .stderr
        .take()
        .context("vulnix stderr was not piped")?;
    let stdout_output = Arc::new(Mutex::new(BoundedOutput::default()));
    let stderr_output = Arc::new(Mutex::new(BoundedOutput::default()));
    let mut stdout_task = tokio::spawn(read_bounded(
        stdout,
        SCANNER_STDOUT_LIMIT,
        Arc::clone(&stdout_output),
    ));
    let mut stderr_task = tokio::spawn(read_bounded(
        stderr,
        SCANNER_STDERR_LIMIT,
        Arc::clone(&stderr_output),
    ));

    let deadline = tokio::time::sleep(timeout);
    tokio::pin!(deadline);
    let status = tokio::select! {
        status = group.wait() => status.context("Failed to wait for vulnix")?,
        _ = &mut deadline => {
            group.terminate().await;
            stdout_task.abort();
            let (stderr, stderr_truncated) =
                capture_terminated_output(&mut stderr_task, &stderr_output).await;
            return Err(VulnixScanExecutionError::with_stderr(
                timeout_error(timeout, stderr_truncated),
                &stderr,
                stderr_truncated,
            ));
        }
    };
    let (stdout, stdout_truncated) = tokio::select! {
        result = finish_output_reader(&mut stdout_task, &stdout_output, "stdout") => result?,
        _ = &mut deadline => {
            group.terminate().await;
            stdout_task.abort();
            stderr_task.abort();
            return Err(anyhow!(
                "vulnix output drain timed out after {} seconds",
                timeout.as_secs()
            )
            .into());
        }
    };
    let (stderr, stderr_truncated) = tokio::select! {
        result = finish_output_reader(&mut stderr_task, &stderr_output, "stderr") => result?,
        _ = &mut deadline => {
            group.terminate().await;
            stderr_task.abort();
            return Err(anyhow!(
                "vulnix output drain timed out after {} seconds",
                timeout.as_secs()
            )
            .into());
        }
    };
    if stdout_truncated {
        group.terminate().await;
        return Err(anyhow!("vulnix stdout exceeded {} bytes", SCANNER_STDOUT_LIMIT).into());
    }
    group.disarm();
    Ok(ScannerCommandOutput {
        status,
        stdout,
        stderr,
        stderr_truncated,
    })
}

async fn read_bounded<R: tokio::io::AsyncRead + Unpin>(
    mut reader: R,
    limit: usize,
    output: SharedBoundedOutput,
) -> std::io::Result<()> {
    // Continue draining after the retained limit so a full child pipe cannot
    // block process exit.
    let mut buffer = [0_u8; 8192];
    loop {
        let count = reader.read(&mut buffer).await?;
        if count == 0 {
            break;
        }
        let mut output = output
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let available = limit.saturating_sub(output.bytes.len());
        output
            .bytes
            .extend_from_slice(&buffer[..count.min(available)]);
        output.truncated |= count > available;
    }
    Ok(())
}

impl Default for VulnixRunner {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::{BoundedOutput, parse_successful_vulnix_output, read_bounded, run_scanner_command};
    use std::sync::{Arc, Mutex};
    use std::time::Duration;
    use tokio::process::Command;

    #[tokio::test]
    async fn bounded_reader_drains_input_after_the_retained_limit() {
        let input = vec![b'x'; 32];
        let output = Arc::new(Mutex::new(BoundedOutput::default()));
        read_bounded(std::io::Cursor::new(input), 8, Arc::clone(&output))
            .await
            .expect("bounded reader should drain an in-memory input");
        let output = output.lock().expect("bounded output lock");
        let retained = output.bytes.clone();
        let truncated = output.truncated;
        assert_eq!(retained, vec![b'x'; 8]);
        assert!(truncated);
    }

    #[tokio::test]
    async fn timeout_retains_stderr_emitted_before_termination() {
        let mut command = Command::new("sh");
        command
            .arg("-c")
            .arg("printf 'local-timeout-secret' >&2; sleep 1");

        let error = run_scanner_command(command, Duration::from_millis(50))
            .await
            .expect_err("fixture must time out");
        assert!(error.to_string().contains("timed out"));
        assert!(error.stderr.contains("local-timeout-secret"));
    }

    #[test]
    fn nonzero_vulnix_exit_returns_stderr_without_parsing_json() {
        let error = parse_successful_vulnix_output(
            Some(1),
            "",
            "vulnix.nix.DeriverLookupError: Cannot determine deriver",
        )
        .expect_err("a nonzero vulnix exit must fail before JSON parsing");

        let message = error.to_string();
        assert!(message.contains("DeriverLookupError"));
        assert!(!message.contains("EOF while parsing"));
    }

    #[test]
    fn successful_exit_parse_failure_retains_stderr() {
        let error = parse_successful_vulnix_output(
            Some(0),
            "not-json",
            "scanner emitted useful parse context",
        )
        .expect_err("malformed successful output must fail");
        assert!(
            error
                .to_string()
                .contains("scanner emitted useful parse context")
        );
    }

    #[test]
    fn malformed_json_from_successful_vulnix_exit_is_a_parse_error() {
        let error = parse_successful_vulnix_output(Some(0), "not-json", "")
            .expect_err("successful malformed output must fail parsing");

        assert!(
            error
                .to_string()
                .contains("Failed to parse vulnix JSON output")
        );
    }

    #[test]
    fn valid_json_from_successful_vulnix_exit_is_accepted() {
        let entries = parse_successful_vulnix_output(Some(0), "[]", "")
            .expect("successful valid output should parse");

        assert!(entries.is_empty());
    }

    #[test]
    fn valid_nonempty_json_from_vulnerability_exit_is_accepted() {
        let json = r#"[{"name":"openssl-3.0.0","pname":"openssl","version":"3.0.0","derivation":"/nix/store/example-openssl.drv","affected_by":["CVE-2026-0001"],"whitelisted":[],"cvssv3_basescore":{"CVE-2026-0001":9.8}}]"#;

        let entries = parse_successful_vulnix_output(Some(2), json, "")
            .expect("exit 2 with valid vulnerability JSON must parse");

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].affected_by, ["CVE-2026-0001"]);
    }

    /// vulnix maps every uncaught `RuntimeError`, including
    /// `DeriverLookupError`, to exit 2 with an empty stdout and the diagnostic
    /// on stderr. Accepting exit 2 unconditionally reported that fatal case as
    /// `EOF while parsing a value at line 1 column 0` and discarded the only
    /// description of the cause, which is what deployed scans reported.
    #[test]
    fn fatal_vulnerability_exit_without_json_reports_stderr() {
        let stderr = "ERROR:vulnix.main:Cannot determine deriver for path \
                      `/nix/store/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa-nixos-system-host`\n\
                      vulnix.nix.DeriverLookupError: Cannot determine deriver";

        let error = parse_successful_vulnix_output(Some(2), "", stderr)
            .expect_err("exit 2 without a JSON report is a fatal vulnix error");

        let message = error.to_string();
        assert!(
            message.contains("DeriverLookupError"),
            "the fatal cause must survive: {message}"
        );
        assert!(
            !message.contains("EOF while parsing"),
            "a fatal vulnix error must not be reported as a JSON parse error: {message}"
        );
    }

    /// Exit 2 with unparseable stdout cannot be a vulnerability report, so it
    /// is treated as the same fatal case rather than as malformed success.
    #[test]
    fn malformed_output_from_vulnerability_exit_reports_process_failure() {
        let error = parse_successful_vulnix_output(Some(2), "not-json", "vulnix exploded")
            .expect_err("exit 2 does not make malformed output valid");

        let message = error.to_string();
        assert!(message.contains("Vulnix scan process failed with exit code 2"));
        assert!(message.contains("vulnix exploded"));
    }

    /// A fatal exit with no stderr must still name the exit status instead of
    /// producing an error with no stated cause.
    #[test]
    fn fatal_exit_without_stderr_still_reports_the_exit_code() {
        let error = parse_successful_vulnix_output(Some(2), "", "")
            .expect_err("exit 2 without a JSON report is a fatal vulnix error");

        let message = error.to_string();
        assert!(message.contains("exit code 2"));
        assert!(message.contains("vulnix produced no stderr output"));
    }
}
