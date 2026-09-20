//! Redacts credentials from builder diagnostics before API serialization.

/// Returns diagnostic text with recognized credentials removed.
///
/// Builder callers MUST apply this policy before they log an external command
/// error or place one in an API request. The server applies its own redaction
/// policy again as defense in depth.
pub fn redact_builder_error(input: &str) -> String {
    input
        .lines()
        .map(|line| {
            let lower_line = line.to_ascii_lowercase();
            if let Some(index) = lower_line.find("authorization:") {
                return format!("{}Authorization: [REDACTED]", &line[..index]);
            }

            line.split_whitespace()
                .map(|token| {
                    let lower = token.to_ascii_lowercase();
                    if lower.contains("password=")
                        || lower.contains("token=")
                        || lower.contains("netrc")
                    {
                        return "[REDACTED]".to_string();
                    }
                    if let Some((scheme, remainder)) = token.split_once("://")
                        && (remainder.contains('@') || remainder.contains('?'))
                    {
                        return format!("{scheme}://[REDACTED]");
                    }
                    token.to_string()
                })
                .collect::<Vec<_>>()
                .join(" ")
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::redact_builder_error;

    #[test]
    fn removes_credentials_from_builder_diagnostics() {
        let redacted = redact_builder_error(
            "Authorization: Bearer secret\nhttps://user:pass@example.test/repo?token=secret password=hunter2 safe-context",
        );

        assert!(redacted.contains("[REDACTED]"));
        assert!(redacted.contains("safe-context"));
        for secret in ["Bearer secret", "user:pass", "token=secret", "hunter2"] {
            assert!(!redacted.contains(secret));
        }
    }
}
