//! Captures Niks3 output privately and produces credential-safe diagnostic hints.
//!
//! Output is untrusted: hints neither establish a root cause nor authorize work.
//! Raw bytes never leave this module through accessors or formatting. Retention
//! uses ordinary private process memory, not locked or guaranteed-zeroized memory.
//! The caller owns process deadlines, kill/reap, and concurrent pipe draining.
//!
//! Supported outer stages come from upstream Niks3
//! [v1.6.0](https://github.com/Mic92/niks3/tree/v1.6.0/client) and
//! [v1.14.2](https://github.com/Mic92/niks3/tree/v1.14.2/client): `upload.go`,
//! `client.go`, `pending_closure.go`, and v1.6.0 `multipart.go`, `nar_task.go`,
//! `metadata_tasks.go`, and `retry.go`. Their `cmd/niks3/main.go` fatal records
//! use the text handler configured by `cmdutil/cmdutil.go`. This is a bounded
//! grammar subset; unsupported wrappers produce `Unknown`.

use std::{fmt, future::poll_fn, pin::Pin, process::ExitStatus};
use tokio::io::{AsyncRead, ReadBuf};

/// Limits retained bytes to 64 KiB per stream, split between prefix and suffix.
///
/// This budget preserves startup context and terminal failures without allowing
/// progress output to grow process memory without a bound.
pub const NIKS3_OUTPUT_LIMIT: usize = 64 * 1024;
const HALF_LIMIT: usize = NIKS3_OUTPUT_LIMIT / 2;
// A complete fatal record must fit this budget; oversized records are unknown.
const TERMINAL_LINE_LIMIT: usize = 4096;

/// Owns private bounded output with summary-only [`Debug`](fmt::Debug) formatting.
///
/// No raw output accessor or serialization is provided. Dropping this value
/// releases memory but does not promise cryptographic erasure.
#[derive(Default)]
pub struct Niks3Output {
    prefix: Vec<u8>,
    suffix: Vec<u8>,
    total: usize,
    read_failed: bool,
}

impl Niks3Output {
    /// Returns the number of privately retained bytes, at most 64 KiB.
    pub fn retained_len(&self) -> usize {
        self.prefix.len() + self.suffix.len()
    }

    /// Returns whether bytes between the retained prefix and suffix were lost.
    pub fn truncated(&self) -> bool {
        self.total > NIKS3_OUTPUT_LIMIT
    }

    /// Returns whether capture stopped because the stream reader failed.
    pub fn output_read_failed(&self) -> bool {
        self.read_failed
    }

    fn retain(&mut self, mut bytes: &[u8]) {
        self.total = self.total.saturating_add(bytes.len());
        let prefix_count = bytes.len().min(HALF_LIMIT - self.prefix.len());
        self.prefix.extend_from_slice(&bytes[..prefix_count]);
        bytes = &bytes[prefix_count..];
        if bytes.len() >= HALF_LIMIT {
            self.suffix.clear();
            self.suffix
                .extend_from_slice(&bytes[bytes.len() - HALF_LIMIT..]);
        } else {
            let discard = (self.suffix.len() + bytes.len()).saturating_sub(HALF_LIMIT);
            self.suffix.drain(..discard);
            self.suffix.extend_from_slice(bytes);
        }
    }

    fn terminal_line(&self) -> Option<Vec<u8>> {
        // INVARIANT: Never join across the discarded middle or classify a
        // partial suffix line as if its beginning were a real record boundary.
        let complete = !self.truncated();
        let mut tail = if complete {
            self.prefix.clone()
        } else {
            Vec::new()
        };
        tail.extend_from_slice(&self.suffix);
        if tail.last() == Some(&b'\n') {
            tail.pop();
        }
        if tail.last() == Some(&b'\r') {
            tail.pop();
        }
        let start = match tail.iter().rposition(|&b| b == b'\n') {
            Some(index) => index + 1,
            None if complete => 0,
            None => return None,
        };
        let line = &tail[start..];
        (line.len() <= TERMINAL_LINE_LIMIT).then(|| line.to_vec())
    }
}

impl fmt::Debug for Niks3Output {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Niks3Output")
            .field("retained_len", &self.retained_len())
            .field("truncated", &self.truncated())
            .field("output_read_failed", &self.read_failed)
            .finish()
    }
}

/// Drains a stream through EOF while retaining only a bounded prefix and suffix.
///
/// Run one collector per pipe concurrently with the child and its deadline.
/// Retention exhaustion does not stop reads. A reader failure returns the
/// partial capture with `output_read_failed()` set; external I/O error text is
/// suppressed. Cancelling this future discards its partial capture. This helper
/// does not impose a deadline or manage the child process.
///
/// # Errors
///
/// The result reserves a static-error channel for callers; currently every
/// completion returns `Ok`, including a read failure represented in metadata.
///
/// # Examples
///
/// ```no_run
/// # async fn example() -> Result<(), &'static str> {
/// use cf_config::niks3_diagnostics::capture_niks3_output;
/// let output = capture_niks3_output(&b"progress without a newline"[..]).await?;
/// assert_eq!(output.retained_len(), 26);
/// # Ok(()) }
/// ```
pub async fn capture_niks3_output<R: AsyncRead + Unpin>(
    mut reader: R,
) -> Result<Niks3Output, &'static str> {
    let mut output = Niks3Output::default();
    let mut scratch = [0_u8; 8192];
    loop {
        let mut buffer = ReadBuf::new(&mut scratch);
        match poll_fn(|cx| Pin::new(&mut reader).poll_read(cx, &mut buffer)).await {
            Ok(()) if buffer.filled().is_empty() => break,
            Ok(()) => output.retain(buffer.filled()),
            Err(_) => {
                output.read_failed = true;
                break;
            }
        }
        // CONCURRENCY: Even an always-ready reader must give the other pipe
        // and the owner's deadline future a chance to run. Self-waking yields
        // do not require Tokio runtime or io-util features.
        let mut yielded = false;
        poll_fn(|cx| {
            if yielded {
                std::task::Poll::Ready(())
            } else {
                yielded = true;
                cx.waker().wake_by_ref();
                std::task::Poll::Pending
            }
        })
        .await;
    }
    Ok(output)
}

/// Identifies a diagnostic hint, never a confirmed cause or authorization result.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Niks3FailureCategory {
    /// Suggests checking server certificate trust on both API and S3 endpoints.
    TlsTrust,
    /// Suggests checking the client identity certificate and matching key.
    ClientCertificate,
    /// Suggests checking credentials after an API-stage HTTP 401 response.
    Unauthorized,
    /// Suggests checking API access policy after an HTTP 403 response.
    Forbidden,
    /// Suggests checking write policy after a signing/completion-stage HTTP 403.
    WritePermission,
    /// Suggests checking the presigned object-upload path, not API token validity.
    PresignedUpload,
    /// Suggests checking DNS, routing, connectivity, and endpoint availability.
    Network,
    /// Suggests checking local Nix store access or path preparation.
    LocalStore,
    /// Reports only a deadline supplied explicitly by the process owner.
    Timeout,
    /// Reports that no supported terminal failure grammar yielded a hint.
    Unknown,
}

/// Contains only safe summary metadata and a static diagnostic category.
///
/// Success does not constitute a push failure: `from_outputs` returns `Unknown`
/// for success. Callers must use exit status for success/failure decisions.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Niks3PushDiagnostic {
    /// Contains an unconfirmed diagnostic hint from supported stderr grammar.
    pub category: Niks3FailureCategory,
    /// Contains the numeric child exit code, absent for signal termination.
    pub exit_code: Option<i32>,
    /// Indicates retention truncation on either output stream.
    pub truncated: bool,
    /// Indicates an output read failure on either stream.
    pub output_read_failed: bool,
}

impl Niks3PushDiagnostic {
    /// Returns a safe hint from failed-process stderr and both stream summaries.
    ///
    /// Stdout is progress-only. Only stderr's last complete, at-most-4-KiB fatal
    /// record is considered. Read failure suppresses classification because EOF
    /// was not established. Unknown versions and malformed records fail closed
    /// to `Unknown`. No output text is returned.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # fn example(status: std::process::ExitStatus,
    /// # stderr: cf_config::niks3_diagnostics::Niks3Output,
    /// # stdout: cf_config::niks3_diagnostics::Niks3Output) {
    /// use cf_config::niks3_diagnostics::Niks3PushDiagnostic;
    /// let hint = Niks3PushDiagnostic::from_outputs(status, &stderr, &stdout);
    /// let safe_message = hint.to_string();
    /// # }
    /// ```
    pub fn from_outputs(
        exit_status: ExitStatus,
        stderr: &Niks3Output,
        stdout: &Niks3Output,
    ) -> Self {
        let mut diagnostic = Self::summary(exit_status.code(), stderr, stdout);
        if !exit_status.success() && !stderr.read_failed {
            diagnostic.category = stderr
                .terminal_line()
                .and_then(|line| fatal_error(&line))
                .map(|error| classify(&error))
                .unwrap_or(Niks3FailureCategory::Unknown);
        }
        diagnostic
    }

    /// Returns a timeout hint only after the caller's process deadline elapsed.
    ///
    /// The caller must still kill/reap the child and settle both collectors.
    /// This constructor does not infer timeout from output or exit code.
    pub fn timeout(exit_code: Option<i32>, stderr: &Niks3Output, stdout: &Niks3Output) -> Self {
        let mut diagnostic = Self::summary(exit_code, stderr, stdout);
        diagnostic.category = Niks3FailureCategory::Timeout;
        diagnostic
    }

    fn summary(exit_code: Option<i32>, stderr: &Niks3Output, stdout: &Niks3Output) -> Self {
        Self {
            category: Niks3FailureCategory::Unknown,
            exit_code,
            truncated: stderr.truncated() || stdout.truncated(),
            output_read_failed: stderr.read_failed || stdout.read_failed,
        }
    }

    /// Returns fixed actionable guidance without any captured output.
    pub fn message(&self) -> &'static str {
        use Niks3FailureCategory::*;
        match self.category {
            TlsTrust => {
                "Niks3 diagnostic hint: check certificate trust, validity, and hostname coverage for both the API and S3 endpoints."
            }
            ClientCertificate => {
                "Niks3 diagnostic hint: check the client certificate/key pair, validity, and accepted mTLS identity for the write API."
            }
            Unauthorized => {
                "Niks3 diagnostic hint: the API reported HTTP 401; check the selected credentials and authentication mode."
            }
            Forbidden => {
                "Niks3 diagnostic hint: the API reported HTTP 403; check identity and cache access policy."
            }
            WritePermission => {
                "Niks3 diagnostic hint: a signing or completion request reported HTTP 403; check cache write permissions and identity."
            }
            PresignedUpload => {
                "Niks3 diagnostic hint: check presigned S3 upload reachability, URL lifetime, and object-storage policy."
            }
            Network => {
                "Niks3 diagnostic hint: check DNS, routing, connectivity, and availability of the API and S3 endpoints."
            }
            LocalStore => {
                "Niks3 diagnostic hint: check local Nix store availability, path existence, and read access."
            }
            Timeout => {
                "Niks3 push exceeded the caller's deadline; check endpoint availability and upload progress."
            }
            Unknown => "Niks3 output suppressed; no supported diagnostic hint is available.",
        }
    }
}

impl fmt::Display for Niks3PushDiagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.message())
    }
}

// COMPATIBILITY: v1.6.0 and v1.14.2 cmd/niks3/main.go emit
// slog.Error("Fatal error", "error", err). cmdutil configures TextHandler.
// Decode only that terminal record. Go-only escapes unsupported by JSON are
// deliberately unknown rather than decoded with lossy or permissive rules.
fn fatal_error(line: &[u8]) -> Option<String> {
    let line = std::str::from_utf8(line).ok()?;
    let timestamped = line.strip_prefix("time=")?;
    let (time, fields) = timestamped.split_once(' ')?;
    chrono::DateTime::parse_from_rfc3339(time).ok()?;
    let encoded = fields.strip_prefix("level=ERROR msg=\"Fatal error\" error=")?;
    let error: String = serde_json::from_str(encoded).ok()?;
    (!error.chars().any(|c| c.is_control())).then_some(error)
}

fn named_cause<'a>(error: &'a str, prefix: &str) -> Option<&'a str> {
    let (name, cause) = error.strip_prefix(prefix)?.split_once(": ")?;
    // IDs/object keys cannot introduce URLs, bodies, or nested cause markers.
    (!name.is_empty()
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-_./".contains(&b)))
    .then_some(cause)
}

fn classify(error: &str) -> Niks3FailureCategory {
    use Niks3FailureCategory::*;
    if let Some(tls) = error.strip_prefix("setting up client TLS: ") {
        if tls.starts_with("loading client certificate: ") {
            return ClientCertificate;
        }
        if tls.starts_with("loading CA certificate: ") || tls.starts_with("parsing CA certificate ")
        {
            return TlsTrust;
        }
        return Unknown;
    }
    if error.starts_with("creating client: getting store directory: ") {
        return LocalStore;
    }
    let Some(push) = error.strip_prefix("pushing paths: ") else {
        return Unknown;
    };
    if [
        "getting path info: ",
        "resolving symlinks: ",
        "preparing closures: ",
    ]
    .iter()
    .any(|prefix| push.starts_with(prefix))
    {
        return LocalStore;
    }
    // SECURITY: Consume known outer stages before inspecting a cause. Never
    // search URLs, XML/JSON response bodies, or arbitrary nested error text.
    let (cause, write_stage, s3) = if let Some(cause) = push
        .strip_prefix("creating pending closures: creating pending closure: ")
        .or_else(|| push.strip_prefix("creating pending closures: creating push: "))
    {
        (cause, false, false)
    } else if let Some(cause) = named_cause(push, "completing pending closure ") {
        (cause, true, false)
    } else if let Some(cause) = push.strip_prefix("signing and uploading narinfos: ") {
        if let Some(cause) = named_cause(cause, "signing narinfos for closure ") {
            (cause, true, false)
        } else if let Some(cause) = named_cause(cause, "uploading narinfo ") {
            (cause, false, true)
        } else {
            return Unknown;
        }
    } else if let Some(objects) = push.strip_prefix("uploading objects: ") {
        if let Some(nar) = named_cause(objects, "uploading NAR ") {
            if let Some(cause) = named_cause(nar, "uploading part ") {
                // v1.6.0 multipart.go puts the S3 HTTP cause below this stage.
                if !nar.strip_prefix("uploading part ").is_some_and(|rest| {
                    rest.split_once(": ").is_some_and(|(part, _)| {
                        !part.is_empty() && part.bytes().all(|b| b.is_ascii_digit())
                    })
                }) {
                    return Unknown;
                }
                return classify_upload_cause(cause, false, true);
            }
            if let Some(cause) = nar.strip_prefix("completing multipart upload: ") {
                return classify_upload_cause(cause, true, false);
            }
            return Unknown;
        }
        let cause = [
            "uploading build log ",
            "uploading realisation ",
            "uploading listing ",
        ]
        .iter()
        .find_map(|prefix| named_cause(objects, prefix));
        let Some(cause) = cause else { return Unknown };
        (cause, false, true)
    } else {
        return Unknown;
    };
    classify_upload_cause(cause, write_stage, s3)
}

fn classify_upload_cause(cause: &str, write_stage: bool, s3: bool) -> Niks3FailureCategory {
    use Niks3FailureCategory::*;
    if let Some(status) = cause.strip_prefix("server returned ") {
        // The colon starts an untrusted response body. Never classify that body.
        let Some((code, _body)) = status.split_once(": ") else {
            return Unknown;
        };
        return match (code, s3, write_stage) {
            ("401" | "403", true, _) => PresignedUpload,
            ("401", false, _) => Unauthorized,
            ("403", false, true) => WritePermission,
            ("403", false, false) => Forbidden,
            (_, true, _) if code.len() == 3 && code.bytes().all(|b| b.is_ascii_digit()) => {
                PresignedUpload
            }
            _ => Unknown,
        };
    }
    if s3
        && cause
            .strip_prefix("unexpected status ")
            .is_some_and(|code| code.len() == 3 && code.bytes().all(|b| b.is_ascii_digit()))
    {
        return PresignedUpload;
    }
    let cause = cause
        .strip_prefix("sending request: ")
        .or_else(|| cause.strip_prefix("uploading: "))
        .unwrap_or(cause);
    let cause = cause
        .strip_prefix("request failed after retries: ")
        .or_else(|| cause.strip_prefix("executing request: "))
        .unwrap_or(cause);
    // net/url.Error formats its URL as a quoted operand. Skip it structurally
    // instead of matching cause-like strings inside a signed query or path.
    let Some(cause) = http_transport_cause(cause) else {
        return Unknown;
    };
    let cause = cause
        .strip_prefix("tls: failed to verify certificate: ")
        .unwrap_or(cause);
    if cause.starts_with("x509: certificate signed by unknown authority")
        || cause.starts_with("x509: certificate has expired or is not yet valid:")
        || cause.starts_with("x509: certificate is valid for ")
        || cause.starts_with("x509: cannot validate certificate for ")
    {
        return TlsTrust;
    }
    if matches!(
        cause,
        "remote error: tls: certificate required"
            | "remote error: tls: bad certificate"
            | "remote error: tls: unknown certificate authority"
    ) {
        return ClientCertificate;
    }
    if cause == "EOF"
        || (cause.starts_with("dial tcp")
            && [
                ": no such host",
                ": connection refused",
                ": network is unreachable",
                ": i/o timeout",
            ]
            .iter()
            .any(|ending| cause.ends_with(ending)))
    {
        return Network;
    }
    Unknown
}

fn http_transport_cause(error: &str) -> Option<&str> {
    let quoted = ["Post ", "Put ", "Get ", "Head "]
        .iter()
        .find_map(|method| error.strip_prefix(method))?;
    if !quoted.starts_with('"') {
        return None;
    }
    let mut escaped = false;
    for (index, byte) in quoted.bytes().enumerate().skip(1) {
        if byte == b'"' && !escaped {
            let url: String = serde_json::from_str(&quoted[..=index]).ok()?;
            let url = url::Url::parse(&url).ok()?;
            if !matches!(url.scheme(), "http" | "https") {
                return None;
            }
            return quoted[index + 1..].strip_prefix(": ");
        }
        escaped = byte == b'\\' && !escaped;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        io,
        task::{Context, Poll},
    };

    fn output(bytes: &[u8]) -> Niks3Output {
        let mut output = Niks3Output::default();
        for chunk in bytes.chunks(8192) {
            output.retain(chunk);
        }
        output
    }

    fn fatal(error: &str) -> Niks3Output {
        output(
            format!(
                "time=2026-10-09T12:00:00Z level=ERROR msg=\"Fatal error\" error={}\n",
                serde_json::to_string(error).unwrap()
            )
            .as_bytes(),
        )
    }

    #[cfg(unix)]
    fn status(code: i32) -> ExitStatus {
        use std::os::unix::process::ExitStatusExt;
        ExitStatus::from_raw(code << 8)
    }

    #[test]
    fn bounded_retention_keeps_terminal_record_without_joining_gap() {
        let mut bytes = vec![b'p'; NIKS3_OUTPUT_LIMIT * 3];
        bytes.extend_from_slice(b"\n");
        let terminal = fatal("setting up client TLS: loading CA certificate: synthetic");
        bytes.extend_from_slice(&terminal.prefix);
        let captured = output(&bytes);
        assert_eq!(captured.retained_len(), NIKS3_OUTPUT_LIMIT);
        assert!(captured.truncated());
        assert_eq!(
            classify(&fatal_error(&captured.terminal_line().unwrap()).unwrap()),
            Niks3FailureCategory::TlsTrust
        );
        assert!(
            output(&vec![b'x'; NIKS3_OUTPUT_LIMIT * 2])
                .terminal_line()
                .is_none()
        );
        assert!(
            output(&vec![b'x'; TERMINAL_LINE_LIMIT + 1])
                .terminal_line()
                .is_none()
        );
    }

    #[test]
    fn verified_stage_categories_and_untrusted_body_barrier() {
        use Niks3FailureCategory::*;
        let cases = [
            (
                "setting up client TLS: loading client certificate: synthetic",
                ClientCertificate,
            ),
            (
                "setting up client TLS: parsing CA certificate \"synthetic\": no PEM certificates found",
                TlsTrust,
            ),
            (
                "creating client: getting store directory: synthetic",
                LocalStore,
            ),
            ("pushing paths: getting path info: synthetic", LocalStore),
            (
                "pushing paths: creating pending closures: creating pending closure: server returned 401: <xml>x509: certificate signed by unknown authority</xml>",
                Unauthorized,
            ),
            (
                "pushing paths: creating pending closures: creating pending closure: server returned 403: synthetic",
                Forbidden,
            ),
            (
                "pushing paths: creating pending closures: creating push: server returned 401: synthetic",
                Unauthorized,
            ),
            (
                "pushing paths: completing pending closure abc-123: server returned 403: synthetic",
                WritePermission,
            ),
            (
                "pushing paths: signing and uploading narinfos: uploading narinfo abc.narinfo: unexpected status 403",
                PresignedUpload,
            ),
            (
                "pushing paths: uploading objects: uploading build log log/abc: server returned 403: synthetic",
                PresignedUpload,
            ),
            (
                "pushing paths: uploading objects: uploading NAR nar/abc.nar.zst: uploading part 1: server returned 403: <xml>x509: certificate signed by unknown authority</xml>",
                PresignedUpload,
            ),
            (
                "pushing paths: uploading objects: uploading NAR nar/abc.nar.zst: completing multipart upload: server returned 403: synthetic",
                WritePermission,
            ),
            (
                "pushing paths: uploading objects: uploading NAR nar/abc.nar.zst: uploading part 1: uploading: request failed after retries: Put \"https://example.invalid/?signature=PRIVATE_SENTINEL\": tls: failed to verify certificate: x509: certificate signed by unknown authority",
                TlsTrust,
            ),
            (
                "pushing paths: uploading objects: uploading NAR nar/abc.nar.zst: uploading part synthetic: server returned 401: synthetic",
                Unknown,
            ),
            (
                "pushing paths: creating pending closures: creating pending closure: sending request: request failed after retries: Post \"https://example.invalid/?401=x509&signature=PRIVATE_SENTINEL\": tls: failed to verify certificate: x509: certificate signed by unknown authority",
                TlsTrust,
            ),
            (
                "pushing paths: creating pending closures: creating pending closure: sending request: request failed after retries: Post \"https://example.invalid/\": dial tcp: lookup example.invalid: no such host",
                Network,
            ),
            (
                "pushing paths: creating pending closures: creating pending closure: sending request: Post \"https://example.invalid/\": remote error: tls: certificate required",
                ClientCertificate,
            ),
            (
                "pushing paths: creating pending closures: creating pending closure: server returned 500: x509: certificate signed by unknown authority",
                Unknown,
            ),
            (
                "pushing paths: creating pending closures: creating pending closure: sending request: Post \"https://example.invalid/?status=401&cause=x509\": synthetic",
                Unknown,
            ),
            (
                "pushing paths: creating pending closures: creating pending closure: server returned 4010: synthetic",
                Unknown,
            ),
            (
                "pushing paths: creating pending closures: creating pending closure: server returned 500: server returned 403: synthetic",
                Unknown,
            ),
            (
                "pushing paths: arbitrary: x509: certificate signed by unknown authority",
                Unknown,
            ),
            ("context deadline exceeded", Unknown),
        ];
        for (error, category) in cases {
            assert_eq!(classify(error), category);
            let captured = fatal(error);
            assert_eq!(
                classify(&fatal_error(&captured.terminal_line().unwrap()).unwrap()),
                category
            );
        }
    }

    #[test]
    #[cfg(unix)]
    fn safe_formatting_stdout_success_read_failure_and_explicit_timeout() {
        let raw = "PRIVATE_SENTINEL Authorization: Bearer synthetic https://example.invalid/?signature=PRIVATE_SENTINEL";
        let stdout = fatal("setting up client TLS: loading CA certificate: PRIVATE_SENTINEL");
        let mut stderr = fatal(raw);
        let diagnostic = Niks3PushDiagnostic::from_outputs(status(1), &stderr, &stdout);
        assert_eq!(diagnostic.category, Niks3FailureCategory::Unknown);
        for safe in [
            format!("{stderr:?}"),
            format!("{diagnostic:?}"),
            diagnostic.to_string(),
        ] {
            assert!(!safe.contains("PRIVATE_SENTINEL"));
            assert!(!safe.contains("Authorization"));
            assert!(!safe.contains("signature="));
        }
        stderr = stdout;
        assert_eq!(
            Niks3PushDiagnostic::from_outputs(status(0), &stderr, &Niks3Output::default()).category,
            Niks3FailureCategory::Unknown
        );
        stderr.read_failed = true;
        let diagnostic =
            Niks3PushDiagnostic::from_outputs(status(1), &stderr, &Niks3Output::default());
        assert_eq!(diagnostic.category, Niks3FailureCategory::Unknown);
        assert!(diagnostic.output_read_failed);
        assert_eq!(
            Niks3PushDiagnostic::timeout(None, &stderr, &Niks3Output::default()).category,
            Niks3FailureCategory::Timeout
        );
    }

    #[test]
    fn malformed_nonterminal_and_invalid_utf8_are_suppressed() {
        let valid = fatal("setting up client TLS: loading CA certificate: synthetic");
        let mut bytes = valid.prefix;
        bytes.extend_from_slice(b"progress\n");
        assert!(fatal_error(&output(&bytes).terminal_line().unwrap()).is_none());
        for bytes in [
            b"\xff".as_slice(),
            b"x509: certificate signed by unknown authority",
            b"time=invalid level=ERROR msg=\"Fatal error\" error=\"synthetic\"",
        ] {
            assert!(fatal_error(bytes).is_none());
        }
    }

    struct BrokenReader;
    impl AsyncRead for BrokenReader {
        fn poll_read(
            self: Pin<&mut Self>,
            _: &mut Context<'_>,
            _: &mut ReadBuf<'_>,
        ) -> Poll<io::Result<()>> {
            Poll::Ready(Err(io::Error::other("PRIVATE_SENTINEL")))
        }
    }

    #[test]
    fn collector_drains_oversized_nonnewline_stream_and_suppresses_io_error() {
        // A ready in-memory AsyncRead needs no runtime or new Tokio features.
        let waker = std::task::Waker::noop();
        let mut cx = Context::from_waker(waker);
        let bytes = vec![b'x'; NIKS3_OUTPUT_LIMIT * 8];
        let mut reader = bytes.as_slice();
        let captured = {
            let mut future = std::pin::pin!(capture_niks3_output(&mut reader));
            loop {
                match future.as_mut().poll(&mut cx) {
                    Poll::Ready(Ok(output)) => break output,
                    Poll::Ready(Err(_)) => panic!("capture must return metadata"),
                    Poll::Pending => continue,
                }
            }
        };
        assert!(reader.is_empty());
        assert!(captured.truncated());
        assert_eq!(captured.retained_len(), NIKS3_OUTPUT_LIMIT);
        let mut future = std::pin::pin!(capture_niks3_output(BrokenReader));
        let Poll::Ready(Ok(captured)) = future.as_mut().poll(&mut cx) else {
            panic!("ready error must complete")
        };
        assert!(captured.output_read_failed());
        assert!(!format!("{captured:?}").contains("PRIVATE_SENTINEL"));
    }

    #[test]
    #[cfg(unix)]
    fn two_backpressured_streams_drain_concurrently_with_terminal_tail() {
        use std::{
            io::{Read, Write},
            os::unix::net::UnixStream,
            time::{Duration, Instant},
        };

        struct NonblockingReader(UnixStream);
        impl AsyncRead for NonblockingReader {
            fn poll_read(
                mut self: Pin<&mut Self>,
                cx: &mut Context<'_>,
                buffer: &mut ReadBuf<'_>,
            ) -> Poll<io::Result<()>> {
                match self.0.read(buffer.initialize_unfilled()) {
                    Ok(count) => {
                        buffer.advance(count);
                        Poll::Ready(Ok(()))
                    }
                    Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                        // This test executor polls again; production readers
                        // register readiness with their own async executor.
                        cx.waker().wake_by_ref();
                        Poll::Pending
                    }
                    Err(error) => Poll::Ready(Err(error)),
                }
            }
        }
        let (out_read, mut out_write) = UnixStream::pair().unwrap();
        let (err_read, mut err_write) = UnixStream::pair().unwrap();
        out_read.set_nonblocking(true).unwrap();
        err_read.set_nonblocking(true).unwrap();
        out_write
            .set_write_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        err_write
            .set_write_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let writer = std::thread::spawn(move || {
            for _ in 0..256 {
                out_write.write_all(&[b'o'; 8192]).unwrap();
                err_write.write_all(&[b'e'; 8192]).unwrap();
            }
            err_write.write_all(b"\n").unwrap();
            let terminal = fatal("setting up client TLS: loading CA certificate: PRIVATE_SENTINEL");
            err_write.write_all(&terminal.prefix).unwrap();
        });
        let mut out_future = std::pin::pin!(capture_niks3_output(NonblockingReader(out_read)));
        let mut err_future = std::pin::pin!(capture_niks3_output(NonblockingReader(err_read)));
        let mut cx = Context::from_waker(std::task::Waker::noop());
        let mut stdout = None;
        let mut stderr = None;
        let deadline = Instant::now() + Duration::from_secs(10);
        while stdout.is_none() || stderr.is_none() {
            assert!(
                Instant::now() < deadline,
                "both collectors must drain through EOF"
            );
            if stdout.is_none() {
                if let Poll::Ready(result) = out_future.as_mut().poll(&mut cx) {
                    stdout = Some(result.unwrap());
                }
            }
            if stderr.is_none() {
                if let Poll::Ready(result) = err_future.as_mut().poll(&mut cx) {
                    stderr = Some(result.unwrap());
                }
            }
            std::thread::yield_now();
        }
        writer.join().unwrap();
        let stdout = stdout.unwrap();
        let stderr = stderr.unwrap();
        assert_eq!(stdout.total, 256 * 8192);
        assert!(stderr.total > 256 * 8192);
        for captured in [&stdout, &stderr] {
            assert_eq!(captured.retained_len(), NIKS3_OUTPUT_LIMIT);
            assert!(captured.truncated());
            assert!(!captured.output_read_failed());
            assert!(!format!("{captured:?}").contains("PRIVATE_SENTINEL"));
        }
        let diagnostic = Niks3PushDiagnostic::from_outputs(status(1), &stderr, &stdout);
        assert_eq!(diagnostic.category, Niks3FailureCategory::TlsTrust);
        assert!(diagnostic.truncated);
    }
}
