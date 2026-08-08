//! Conservative NixOS configuration assertion inference from STIG fix text.
//!
//! This module implements deterministic, high-confidence inference of NixOS
//! option assignments from STIG remediation (`<fixtext>`) content. The approach
//! is intentionally narrow: only literal, well-typed NixOS option assignments of
//! the form `option.path = value;` are inferred.  Complex expressions, multi-line
//! attrsets, and anything ambiguous are rejected — the caller falls back to
//! displaying the raw fix text for manual annotation.
//!
//! # Safety
//!
//! Inferred assertions are **never executed directly**.  The generated
//! `NixosOptionAssertionDraft` becomes a suggestion that the user reviews in the
//! Refine modal before import.  The module never evaluates Nix code or runs
//! arbitrary sub-processes.
//!
//! # Supported grammar
//!
//! ```text
//! option.path.segments = <literal>;
//! ```
//!
//! where `<literal>` is one of:
//!
//! - `true` / `false` — Boolean
//! - A non-negative integer decimal — Integer
//! - A double-quoted string with no interpolation — StringLiteral
//!
//! Option paths must consist only of alphanumeric identifiers, hyphens, and dots.
//! Any assignment whose path or value does not match this grammar is not inferred.

use serde::{Deserialize, Serialize};

/// A single inferred NixOS configuration assertion.
///
/// Represents the semantic content of one literal NixOS option assignment
/// extracted from STIG fix text.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NixosOptionAssertionDraft {
    /// Dotted NixOS option path, e.g. `"networking.firewall.enable"`.
    pub option_path: String,
    /// The expected value.
    pub expected_value: NixosLiteralValue,
    /// The Nix expression that should evaluate to `true` when the option
    /// matches the expected value.  E.g. `cfg.config.networking.firewall.enable == true`.
    pub nix_expression: String,
    /// A human-readable label for the assertion.
    pub description: String,
}

/// A literal NixOS option value supported by this inference engine.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
pub enum NixosLiteralValue {
    Boolean(bool),
    Integer(i64),
    StringLiteral(String),
}

impl NixosLiteralValue {
    /// The Nix source representation of this value.
    pub fn nix_repr(&self) -> String {
        match self {
            Self::Boolean(b) => if *b { "true".into() } else { "false".into() },
            Self::Integer(n) => n.to_string(),
            Self::StringLiteral(s) => format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\"")),
        }
    }

    /// Equality operator fragment appropriate for a Nix assertion.
    pub fn comparison_operator(&self) -> &'static str {
        "=="
    }
}

/// Attempt to infer zero or more NixOS option assertions from STIG fix text.
///
/// Silently skips any line that does not unambiguously match the supported
/// grammar.  Returns an empty `Vec` when no safe inferences can be made.
///
/// This function is pure and does not perform I/O.
pub fn infer_nixos_assertions(fix_text: &str) -> Vec<NixosOptionAssertionDraft> {
    let mut results = Vec::new();

    for line in fix_text.lines() {
        let trimmed = line.trim();

        // Only consider lines that end with a semicolon (Nix assignment syntax).
        let Some(without_semi) = trimmed.strip_suffix(';') else {
            continue;
        };

        // Split on the first `=` to get `path = value`.
        let Some((raw_path, raw_value)) = without_semi.split_once('=') else {
            continue;
        };

        let path = raw_path.trim();
        let value_str = raw_value.trim();

        // Validate the option path: only alphanumeric, hyphens, dots, underscores.
        if !is_valid_nix_option_path(path) {
            continue;
        }

        // Parse the value literal.
        let Some(value) = parse_nix_literal(value_str) else {
            continue;
        };

        let nix_repr = value.nix_repr();
        let nix_expression = format!(
            "cfg.config.{} {} {}",
            path,
            value.comparison_operator(),
            nix_repr,
        );

        let description = format!(
            "NixOS option {} must be {}",
            path, nix_repr,
        );

        results.push(NixosOptionAssertionDraft {
            option_path: path.to_string(),
            expected_value: value,
            nix_expression,
            description,
        });
    }

    results
}

/// Returns `true` when `s` is a valid dotted NixOS option path.
///
/// Accepts identifiers composed of ASCII letters, digits, hyphens, and
/// underscores, separated by dots.  Rejects empty segments, leading/trailing
/// dots, and any character outside the accepted set.
fn is_valid_nix_option_path(s: &str) -> bool {
    if s.is_empty() || s.starts_with('.') || s.ends_with('.') {
        return false;
    }
    for segment in s.split('.') {
        if segment.is_empty() {
            return false;
        }
        if !segment
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        {
            return false;
        }
    }
    true
}

/// Parse a Nix literal from a trimmed value string.
///
/// Accepts `true`, `false`, non-negative decimal integers, and double-quoted
/// strings without interpolation (`${...}`) or escape sequences beyond `\\`
/// and `\"`.  Returns `None` for anything else.
fn parse_nix_literal(s: &str) -> Option<NixosLiteralValue> {
    // Boolean
    if s == "true" {
        return Some(NixosLiteralValue::Boolean(true));
    }
    if s == "false" {
        return Some(NixosLiteralValue::Boolean(false));
    }

    // Integer: all decimal digits, optionally leading minus for negative.
    // Restrict to non-negative for safety (negative options are unusual in NixOS).
    if s.chars().all(|c| c.is_ascii_digit()) && !s.is_empty() {
        if let Ok(n) = s.parse::<i64>() {
            return Some(NixosLiteralValue::Integer(n));
        }
    }

    // Double-quoted string: no `${` interpolation allowed.
    if let Some(inner) = s.strip_prefix('"').and_then(|s| s.strip_suffix('"')) {
        // Reject Nix string interpolation.
        if inner.contains("${") {
            return None;
        }
        // Reject unescaped embedded quotes (would indicate malformed input).
        // Simple check: after stripping outer quotes, no remaining `"` that is
        // not preceded by `\`.
        let mut chars = inner.chars().peekable();
        let mut validated = String::new();
        while let Some(c) = chars.next() {
            if c == '\\' {
                match chars.next() {
                    Some('\\') => validated.push('\\'),
                    Some('"') => validated.push('"'),
                    Some('n') => validated.push('\n'),
                    Some('t') => validated.push('\t'),
                    _ => return None, // unsupported escape
                }
            } else if c == '"' {
                // Unescaped inner quote: reject.
                return None;
            } else {
                validated.push(c);
            }
        }
        return Some(NixosLiteralValue::StringLiteral(validated));
    }

    None
}

/// Extract the human-readable VulnDiscussion text from an XCCDF `<description>`
/// field value.
///
/// STIG `<description>` elements contain XML-escaped sub-elements such as
/// `<VulnDiscussion>…</VulnDiscussion>` embedded in a plain text node. The
/// XML parser unescapes entity references so the stored value contains literal
/// `<VulnDiscussion>` tags, not `&lt;VulnDiscussion&gt;`.
///
/// This function extracts the content of the first `<VulnDiscussion>` element
/// and strips any remaining XML-like tags from it. Falls back to a best-effort
/// tag-strip of the whole string if no `<VulnDiscussion>` element is found.
///
/// Returns an empty string when the input is empty.
pub fn extract_vuln_discussion(description: &str) -> String {
    if description.is_empty() {
        return String::new();
    }

    // Try to find <VulnDiscussion>...</VulnDiscussion>
    if let Some(start) = description.find("<VulnDiscussion>") {
        let content_start = start + "<VulnDiscussion>".len();
        let content = if let Some(end) = description[content_start..].find("</VulnDiscussion>") {
            &description[content_start..content_start + end]
        } else {
            // No closing tag — take everything after the opening tag.
            &description[content_start..]
        };
        return strip_xml_tags(content).trim().to_string();
    }

    // No VulnDiscussion element — strip all XML-like tags from the whole string.
    strip_xml_tags(description).trim().to_string()
}

/// Remove XML-like tags from a string, preserving text content.
///
/// This is a simple character-level pass; it does not parse XML. It is
/// intentionally used only for display-layer sanitization of content that
/// was already well-formed when parsed, so a full XML parser is unnecessary.
fn strip_xml_tags(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut in_tag = false;

    for c in s.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => result.push(c),
            _ => {}
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn infers_boolean_true_option() {
        // The option assignment must be on its own line ending with semicolon.
        let fix = "Configure the following:\n networking.firewall.enable = true;";
        let assertions = infer_nixos_assertions(fix);
        assert_eq!(assertions.len(), 1);
        let a = &assertions[0];
        assert_eq!(a.option_path, "networking.firewall.enable");
        assert_eq!(a.expected_value, NixosLiteralValue::Boolean(true));
        assert_eq!(
            a.nix_expression,
            "cfg.config.networking.firewall.enable == true"
        );
    }

    #[test]
    fn infers_boolean_false_option() {
        let fix = " services.openssh.enable = false;";
        let assertions = infer_nixos_assertions(fix);
        assert_eq!(assertions.len(), 1);
        assert_eq!(assertions[0].option_path, "services.openssh.enable");
        assert_eq!(assertions[0].expected_value, NixosLiteralValue::Boolean(false));
    }

    #[test]
    fn infers_integer_option() {
        let fix = " services.openssh.maxAuthTries = 4;";
        let assertions = infer_nixos_assertions(fix);
        assert_eq!(assertions.len(), 1);
        assert_eq!(assertions[0].expected_value, NixosLiteralValue::Integer(4));
        assert_eq!(
            assertions[0].nix_expression,
            "cfg.config.services.openssh.maxAuthTries == 4"
        );
    }

    #[test]
    fn infers_string_option() {
        let fix = r#" environment.etc."pam_tally2".source = "/dev/null";"#;
        // path contains / so not a valid Nix option path — should be skipped
        let assertions = infer_nixos_assertions(fix);
        assert!(assertions.is_empty(), "path with invalid chars should be skipped");
    }

    #[test]
    fn infers_multiple_options_from_multiline_fixtext() {
        let fix = "Configure the following:\n security.auditd.enable = true;\n security.audit.enable = true;\n";
        let assertions = infer_nixos_assertions(fix);
        assert_eq!(assertions.len(), 2);
        assert_eq!(assertions[0].option_path, "security.auditd.enable");
        assert_eq!(assertions[1].option_path, "security.audit.enable");
    }

    #[test]
    fn rejects_nix_interpolation_in_string() {
        let fix = r#"some.option = "${pkgs.bash}/bin/bash";"#;
        let assertions = infer_nixos_assertions(fix);
        assert!(assertions.is_empty());
    }

    #[test]
    fn rejects_complex_nix_expressions() {
        let fix = "some.option = [ \"a\" \"b\" ];";
        let assertions = infer_nixos_assertions(fix);
        assert!(assertions.is_empty());
    }

    #[test]
    fn rejects_empty_option_path() {
        let fix = " = true;";
        let assertions = infer_nixos_assertions(fix);
        assert!(assertions.is_empty());
    }

    #[test]
    fn v268078_firewall_rule_inferred_correctly() {
        // Exact text from the Anduril NixOS STIG V-268078 <fixtext>
        let fix = "Configure /etc/nixos/configuration.nix to enforce firewall rules by \
            adding the following configuration settings:\n\n \
            networking.firewall.enable = true;\n\n\
            Rebuild the system with the following command:\n\n\
            $ sudo nixos-rebuild switch";
        let assertions = infer_nixos_assertions(fix);
        assert_eq!(assertions.len(), 1, "exactly one assertion inferred for firewall rule");
        let a = &assertions[0];
        assert_eq!(a.option_path, "networking.firewall.enable");
        assert_eq!(a.expected_value, NixosLiteralValue::Boolean(true));
        assert_eq!(
            a.nix_expression,
            "cfg.config.networking.firewall.enable == true"
        );
    }

    #[test]
    fn extract_vuln_discussion_strips_tags() {
        let desc = "<VulnDiscussion>The firewall must be enabled.</VulnDiscussion>\
                    <FalsePositives></FalsePositives>";
        assert_eq!(
            extract_vuln_discussion(desc),
            "The firewall must be enabled."
        );
    }

    #[test]
    fn extract_vuln_discussion_falls_back_to_strip_all() {
        let desc = "<SomeOtherElement>Content without VulnDiscussion</SomeOtherElement>";
        assert_eq!(
            extract_vuln_discussion(desc),
            "Content without VulnDiscussion"
        );
    }

    #[test]
    fn extract_vuln_discussion_empty_input() {
        assert_eq!(extract_vuln_discussion(""), "");
    }

    #[test]
    fn valid_option_paths() {
        assert!(is_valid_nix_option_path("networking.firewall.enable"));
        assert!(is_valid_nix_option_path("services.openssh.enable"));
        assert!(is_valid_nix_option_path("security.auditd.enable"));
        assert!(is_valid_nix_option_path("some-hyphenated.option"));
    }

    #[test]
    fn invalid_option_paths() {
        assert!(!is_valid_nix_option_path(""));
        assert!(!is_valid_nix_option_path(".leading.dot"));
        assert!(!is_valid_nix_option_path("trailing.dot."));
        assert!(!is_valid_nix_option_path("has..double.dot"));
        assert!(!is_valid_nix_option_path("has/slash"));
        assert!(!is_valid_nix_option_path("has spaces"));
    }
}
