//! Redacts sensitive snapshot data before persistence, indexing, or logging.
//!
//! Snapshot callers must apply this module before they calculate content
//! digests or searchable text. A caller must not retain the unredacted value
//! as an alternate field after this boundary.

use serde_json::{Map, Value};
use url::Url;

/// Is the replacement for a value that can contain a secret.
pub const REDACTED_VALUE: &str = "[REDACTED]";

/// Returns a safe evaluator diagnostic while preserving useful failure detail.
///
/// The deterministic text policy removes credentials, provider tokens, JWTs,
/// and high-entropy token-like words. Ordinary error text remains available.
pub fn redact_evaluation_error(input: &str) -> String {
    redact_text(input)
}

/// Returns a copy of an option value after applying the safe-value policy.
///
/// Ordinary strings and typed package metadata remain available. Sensitive
/// attribute names are removed with their values. Credential-bearing strings,
/// provider tokens, JWTs, and high-entropy token-like values are redacted.
pub fn redact_typed_value(value: &Value) -> Value {
    redact_typed_value_for_path(value, false)
}

/// Returns a safe option value using its full option path as policy context.
pub fn redact_option_value(path: &str, value: &Value) -> Value {
    redact_typed_value_for_path(value, is_sensitive_path(path))
}

fn redact_typed_value_for_path(value: &Value, sensitive_context: bool) -> Value {
    match value {
        Value::Null | Value::Bool(_) | Value::Number(_) if sensitive_context => {
            Value::String(REDACTED_VALUE.to_string())
        }
        Value::Null | Value::Bool(_) | Value::Number(_) => value.clone(),
        Value::String(value) => Value::String(if sensitive_context {
            REDACTED_VALUE.to_string()
        } else {
            redact_text(value)
        }),
        Value::Array(items) => Value::Array(
            items
                .iter()
                .map(|value| redact_typed_value_for_path(value, sensitive_context))
                .collect(),
        ),
        Value::Object(fields) => {
            let mut redacted = Map::new();
            for (key, value) in fields {
                if is_sensitive_field(key) {
                    redacted.insert(
                        REDACTED_VALUE.to_string(),
                        Value::String(REDACTED_VALUE.to_string()),
                    );
                } else {
                    let value = if key == "kind" {
                        value
                            .as_str()
                            .filter(|kind| {
                                matches!(
                                    *kind,
                                    "scalar"
                                        | "package"
                                        | "list"
                                        | "attribute_set"
                                        | "submodule"
                                        | "opaque"
                                        | "failed"
                                )
                            })
                            .map_or_else(
                                || Value::String(REDACTED_VALUE.to_string()),
                                |kind| Value::String(kind.to_string()),
                            )
                    } else {
                        redact_typed_value_for_path(value, sensitive_context)
                    };
                    redacted.insert(key.clone(), value);
                }
            }
            Value::Object(redacted)
        }
    }
}

/// Returns a recursively redacted copy of a JSON value.
///
/// Object fields with sensitive names are replaced in full. Other strings are
/// scanned for credential-bearing URLs, authorization headers, and common
/// secret assignments. The input value is not modified.
pub fn redact_json(value: &Value) -> Value {
    match value {
        Value::Array(items) => Value::Array(items.iter().map(redact_json).collect()),
        Value::Object(fields) => Value::Object(redact_object(fields)),
        Value::String(value) => Value::String(redact_text(value)),
        scalar => scalar.clone(),
    }
}

/// Returns a redacted flake-output payload, including path-sensitive defaults.
///
/// Exported-module defaults use their declaration path as the sensitivity
/// context. This prevents a scalar default such as `services.x.password` from
/// bypassing object-key redaction merely because the serialized field is named
/// `default`.
pub fn redact_flake_output(value: &Value) -> Value {
    let mut redacted = redact_json(value);
    let Some(modules) = redacted
        .get_mut("exported_modules")
        .and_then(Value::as_array_mut)
    else {
        return redacted;
    };
    for module in modules.iter_mut().filter_map(Value::as_object_mut) {
        if let Some(source_path) = module.get_mut("source_path") {
            *source_path = source_path
                .as_str()
                .and_then(normalize_carrier_source_path)
                .map_or(Value::Null, Value::String);
        }
    }
    for declaration in modules
        .iter_mut()
        .filter_map(Value::as_object_mut)
        .filter_map(|module| module.get_mut("declarations"))
        .filter_map(Value::as_array_mut)
        .flatten()
    {
        let Some(declaration) = declaration.as_object_mut() else {
            continue;
        };
        if declaration.get("has_default").and_then(Value::as_bool) == Some(true) {
            let path = declaration
                .get("path")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            if let Some(default) = declaration.get_mut("default") {
                *default = redact_option_value(&path, default);
            }
        }
    }
    redact_error_fields(&mut redacted);
    redacted
}

fn normalize_carrier_source_path(path: &str) -> Option<String> {
    let path = path.trim().replace('\\', "/");
    if path.is_empty() || path.starts_with('/') {
        return None;
    }
    let mut components = Vec::new();
    for component in path.split('/') {
        match component {
            "" | "." => {}
            ".." => return None,
            value => components.push(value),
        }
    }
    (!components.is_empty())
        .then(|| components.join("/"))
        .or_else(|| (path == ".").then(|| ".".to_string()))
}

fn redact_error_fields(value: &mut Value) {
    match value {
        Value::Array(values) => values.iter_mut().for_each(redact_error_fields),
        Value::Object(fields) => {
            for (key, value) in fields {
                if key == "error" || key.ends_with("_error") {
                    if let Some(error) = value.as_str() {
                        *value = Value::String(redact_evaluation_error(error));
                    } else if !value.is_null() {
                        *value = redact_json(value);
                    }
                } else {
                    redact_error_fields(value);
                }
            }
        }
        _ => {}
    }
}

/// Returns text with recognized credentials and secret assignments removed.
///
/// This function preserves non-sensitive text for diagnostics and search. It
/// removes URL query strings and fragments because repository URLs can carry
/// tokens under provider-specific parameter names.
pub fn redact_text(input: &str) -> String {
    let mut output = String::with_capacity(input.len());

    for line in input.split_inclusive('\n') {
        let (body, newline) = line
            .strip_suffix('\n')
            .map_or((line, ""), |body| (body, "\n"));

        if let Some((prefix, _)) = split_authorization_header(body) {
            output.push_str(prefix);
            output.push_str(REDACTED_VALUE);
        } else {
            output.push_str(&redact_token_like_words(&redact_line(body)));
        }
        output.push_str(newline);
    }

    output
}

fn redact_object(fields: &Map<String, Value>) -> Map<String, Value> {
    let mut redacted = Map::new();
    for (key, value) in fields {
        if is_sensitive_field(key) {
            redacted.insert(
                REDACTED_VALUE.to_string(),
                Value::String(REDACTED_VALUE.to_string()),
            );
        } else {
            redacted.insert(key.clone(), redact_json(value));
        }
    }
    redacted
}

fn split_authorization_header(line: &str) -> Option<(&str, &str)> {
    let separator = line.find(':')?;
    let (name, value) = line.split_at(separator + 1);
    if name[..name.len() - 1]
        .trim()
        .eq_ignore_ascii_case("authorization")
    {
        let value = value.trim_start();
        let prefix_len = line.len() - value.len();
        Some((&line[..prefix_len], value))
    } else {
        None
    }
}

fn redact_line(line: &str) -> String {
    let mut output = String::with_capacity(line.len());
    let mut cursor = 0;

    while cursor < line.len() {
        let remaining = &line[cursor..];
        if has_url_scheme(remaining) {
            let end = remaining.find(is_url_terminator).unwrap_or(remaining.len());
            output.push_str(&redact_url(&remaining[..end]));
            cursor += end;
            continue;
        }

        if let Some((consumed, replacement)) = redact_scp_style_url(remaining) {
            output.push_str(&replacement);
            cursor += consumed;
            continue;
        }

        if let Some((consumed, replacement)) = redact_bearer_or_token(remaining) {
            output.push_str(&replacement);
            cursor += consumed;
            continue;
        }

        if let Some((consumed, replacement)) = redact_assignment(remaining) {
            output.push_str(&replacement);
            cursor += consumed;
            continue;
        }

        let Some(character) = remaining.chars().next() else {
            break;
        };
        output.push(character);
        cursor += character.len_utf8();
    }

    output
}

fn has_url_scheme(input: &str) -> bool {
    let Some(separator) = input.find("://") else {
        return false;
    };
    let scheme = &input[..separator];
    let mut characters = scheme.chars();
    characters
        .next()
        .is_some_and(|character| character.is_ascii_alphabetic())
        && characters.all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '+' | '-' | '.')
        })
}

fn redact_scp_style_url(input: &str) -> Option<(usize, String)> {
    let end = input.find(is_url_terminator).unwrap_or(input.len());
    let candidate = &input[..end];
    let at = candidate.find('@')?;
    let host_path = &candidate[at + 1..];
    let colon = host_path.find(':')?;
    if at == 0
        || colon == 0
        || host_path[colon + 1..].is_empty()
        || candidate[..at].contains('/')
        || host_path[..colon].contains('/')
    {
        return None;
    }
    Some((end, format!("REDACTED@{host_path}")))
}

fn redact_bearer_or_token(input: &str) -> Option<(usize, String)> {
    let word_end = input
        .find(|character: char| !character.is_ascii_alphanumeric() && character != '_')
        .unwrap_or(input.len());
    let word = &input[..word_end];
    let is_bearer = word.eq_ignore_ascii_case("bearer");
    let is_token = word.eq_ignore_ascii_case("token")
        || word.eq_ignore_ascii_case("access_token")
        || word.eq_ignore_ascii_case("auth_token");
    if !is_bearer && !is_token {
        return None;
    }
    let after_word = &input[word_end..];
    let whitespace = after_word.len() - after_word.trim_start().len();
    if whitespace == 0 {
        return None;
    }
    let value_index = word_end + whitespace;
    let value_len = secret_value_len(&input[value_index..]);
    if value_len == 0 {
        return None;
    }
    Some((
        value_index + value_len,
        format!(
            "{}{}{}",
            &input[..word_end],
            &input[word_end..value_index],
            REDACTED_VALUE
        ),
    ))
}

fn redact_assignment(input: &str) -> Option<(usize, String)> {
    let key_end = input
        .find(|character: char| !character.is_ascii_alphanumeric() && character != '_')
        .unwrap_or(input.len());
    if key_end == 0 || !is_sensitive_field(&input[..key_end]) {
        return None;
    }

    let after_key = &input[key_end..];
    let whitespace = after_key.len() - after_key.trim_start().len();
    let separator_index = key_end + whitespace;
    let separator = input[separator_index..].chars().next()?;
    if !matches!(separator, '=' | ':') {
        return None;
    }

    let value_start = separator_index + separator.len_utf8();
    let after_separator = &input[value_start..];
    let value_whitespace = after_separator.len() - after_separator.trim_start().len();
    let value_index = value_start + value_whitespace;
    let value_len = secret_value_len(&input[value_index..]);
    if value_len == 0 {
        return None;
    }

    let consumed = value_index + value_len;
    let mut replacement = input[..value_index].to_string();
    replacement.push_str(REDACTED_VALUE);
    Some((consumed, replacement))
}

fn secret_value_len(value: &str) -> usize {
    let Some(first) = value.chars().next() else {
        return 0;
    };

    if matches!(first, '\'' | '"') {
        let quote_len = first.len_utf8();
        return value[quote_len..]
            .find(first)
            .map_or(value.len(), |end| quote_len + end + quote_len);
    }

    value
        .find(|character: char| character.is_whitespace() || matches!(character, ',' | ';'))
        .unwrap_or(value.len())
}

fn redact_url(input: &str) -> String {
    let (git_prefix, parse_input) = input
        .strip_prefix("git+")
        .map_or(("", input), |value| ("git+", value));
    let Ok(mut url) = Url::parse(parse_input) else {
        return input.to_string();
    };

    let has_user_info = !url.username().is_empty() || url.password().is_some();
    if has_user_info {
        let _ = url.set_username("REDACTED");
        let _ = url.set_password(None);
    }
    url.set_query(None);
    url.set_fragment(None);
    format!("{git_prefix}{url}")
}

fn redact_token_like_words(input: &str) -> String {
    let mut output = String::with_capacity(input.len());
    let mut cursor = 0;
    while cursor < input.len() {
        let remaining = &input[cursor..];
        let Some(character) = remaining.chars().next() else {
            break;
        };
        if is_token_character(character) {
            let end = remaining
                .find(|candidate: char| !is_token_character(candidate))
                .unwrap_or(remaining.len());
            let candidate = &remaining[..end];
            if is_sensitive_token(candidate) {
                output.push_str(REDACTED_VALUE);
            } else {
                output.push_str(candidate);
            }
            cursor += end;
        } else {
            output.push(character);
            cursor += character.len_utf8();
        }
    }
    output
}

fn is_token_character(character: char) -> bool {
    character.is_ascii_alphanumeric() || matches!(character, '_' | '-' | '.' | '+' | '/' | '=')
}

fn is_sensitive_token(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    let provider_prefix = lower.starts_with("github_pat_")
        || lower.starts_with("ghp_")
        || lower.starts_with("gho_")
        || lower.starts_with("ghu_")
        || lower.starts_with("ghs_")
        || lower.starts_with("ghr_")
        || lower.starts_with("glpat-")
        || lower.starts_with("xoxb-")
        || lower.starts_with("xoxp-")
        || lower.starts_with("sk_live_")
        || lower.starts_with("sk_test_")
        || value.starts_with("AKIA")
        || value.starts_with("ASIA")
        || value.starts_with("AIza");
    let jwt = value.len() >= 32
        && value.split('.').count() == 3
        && value
            .split('.')
            .all(|part| part.len() >= 8 && part.chars().all(is_base64url_character));
    provider_prefix || jwt || is_high_entropy_token(value)
}

fn is_base64url_character(character: char) -> bool {
    character.is_ascii_alphanumeric() || matches!(character, '_' | '-')
}

fn is_high_entropy_token(value: &str) -> bool {
    if value.len() < 32
        || value.len() > 512
        || value.starts_with("/nix/store/")
        || (matches!(value.len(), 40 | 64)
            && value.chars().all(|character| character.is_ascii_hexdigit()))
        || (value.len() == 32
            && value
                .chars()
                .all(|character| "0123456789abcdfghijklmnpqrsvwxyz".contains(character)))
    {
        return false;
    }
    let classes = [
        value
            .chars()
            .any(|character| character.is_ascii_lowercase()),
        value
            .chars()
            .any(|character| character.is_ascii_uppercase()),
        value.chars().any(|character| character.is_ascii_digit()),
        value
            .chars()
            .any(|character| matches!(character, '_' | '-' | '+' | '/' | '=')),
    ];
    classes.into_iter().filter(|present| *present).count() >= 3
        && value.chars().all(is_token_character)
}

fn is_url_terminator(character: char) -> bool {
    character.is_whitespace() || matches!(character, '\'' | '"' | ')' | ']' | '>')
}

fn is_sensitive_field(field: &str) -> bool {
    // SECURITY: Removing separators makes camelCase, snake_case, kebab-case,
    // and dotted names share one comparison form. Secret classification must
    // not depend on a producer's naming convention.
    let normalized = field
        .chars()
        .filter(|character| character.is_ascii_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect::<String>();

    normalized.contains("password")
        || normalized.contains("passwd")
        || normalized.contains("secret")
        || normalized.contains("token")
        || normalized.contains("credential")
        || normalized == "authorization"
        || normalized == "auth"
        || normalized == "apikey"
        || normalized == "privatekey"
        || normalized == "accesskey"
        || normalized == "passphrase"
        || normalized == "signingkey"
        || normalized == "netrc"
        || normalized == "netrcfile"
        || normalized == "gitaskpass"
        || normalized == "pat"
        || normalized.ends_with("pat")
}

fn is_sensitive_path(path: &str) -> bool {
    path.split(['.', '/', '[', ']']).any(is_sensitive_field)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{
        REDACTED_VALUE, redact_flake_output, redact_json, redact_option_value, redact_text,
        redact_typed_value,
    };

    #[test]
    fn recursively_redacts_sensitive_fields_and_nested_urls() {
        let input = json!({
            "password": "hunter2",
            "nested": [{
                "access-token": "abc123",
                "source": "https://user:pass@example.com/repo.git?token=query-secret#fragment"
            }],
            "public_key": "ssh-ed25519 public-material",
            "enabled": true
        });

        let redacted = redact_json(&input);

        assert!(redacted.get("password").is_none());
        assert!(redacted["nested"][0].get("access-token").is_none());
        assert_eq!(
            redacted["nested"][0]["source"],
            "https://REDACTED@example.com/repo.git"
        );
        assert_eq!(redacted["public_key"], "ssh-ed25519 public-material");
        assert_eq!(redacted["enabled"], true);
        let serialized = redacted.to_string();
        for secret in ["hunter2", "abc123", "user:pass", "query-secret"] {
            assert!(!serialized.contains(secret));
        }
    }

    #[test]
    fn redacts_headers_assignments_and_quoted_values() {
        let input =
            "Authorization: Bearer top-secret\napi_key=abc password: 'two words' safe=value";

        let redacted = redact_text(input);

        assert_eq!(
            redacted,
            "Authorization: [REDACTED]\napi_key=[REDACTED] password: [REDACTED] safe=value"
        );
        assert!(!redacted.contains("top-secret"));
        assert!(!redacted.contains("two words"));
    }

    #[test]
    fn preserves_non_sensitive_diagnostic_text() {
        let input = "evaluation failed for services.openssh.enable at module.nix:42";

        assert_eq!(redact_text(input), input);
    }

    #[test]
    fn redacts_credential_urls_in_evaluator_diagnostics() {
        let input = "fetch failed: git+https://build-user:p%40ss@example.test/repo?access_token=raw#frag\nssh://deploy:ssh-secret@example.test/repo?token=query\ngit-user@git.example.test:private/repo.git\nBearer diagnostic-secret; token another-secret";
        let redacted = redact_text(input);
        assert_eq!(
            redacted,
            "fetch failed: git+https://REDACTED@example.test/repo\nssh://REDACTED@example.test/repo\nREDACTED@git.example.test:private/repo.git\nBearer [REDACTED]; token [REDACTED]"
        );
        for secret in [
            "build-user",
            "p%40ss",
            "access_token",
            "raw",
            "frag",
            "deploy",
            "ssh-secret",
            "query",
            "git-user",
            "diagnostic-secret",
            "another-secret",
        ] {
            assert!(!redacted.contains(secret));
        }
    }

    #[test]
    fn redacts_credentials_from_arbitrary_url_schemes() {
        let input = "git://git-user:git-secret@example.test/repo?token=query#fragment postgresql://db-user:db-secret@db.example.test/app?sslmode=require";

        let redacted = redact_text(input);

        assert_eq!(
            redacted,
            "git://REDACTED@example.test/repo postgresql://REDACTED@db.example.test/app"
        );
        for secret in [
            "git-user",
            "git-secret",
            "query",
            "fragment",
            "db-user",
            "db-secret",
            "sslmode",
        ] {
            assert!(!redacted.contains(secret));
        }
    }

    #[test]
    fn recursively_redacts_credential_bearing_lock_metadata() {
        let lock = json!({
            "nodes": {
                "root": {"locked": {"url": "ssh://git:secret@example.test/repo?token=raw"}},
                "dep": {"original": {"url": "user@git.example.test:private/dep.git"}}
            }
        });

        let redacted = redact_json(&lock).to_string();
        for secret in ["git:secret", "token", "raw", "user@"] {
            assert!(!redacted.contains(secret));
        }
        assert!(redacted.contains("ssh://REDACTED@example.test/repo"));
        assert!(redacted.contains("REDACTED@git.example.test:private/dep.git"));
    }

    #[test]
    fn declaration_path_redacts_scalar_exported_module_default() {
        let payload = json!({
            "exported_modules": [{
                "declarations": [{
                    "path": "services.example.apiToken",
                    "has_default": true,
                    "default": {"kind": "scalar", "value": "module-secret"}
                }]
            }]
        });
        let redacted = redact_flake_output(&payload);
        assert_eq!(
            redacted["exported_modules"][0]["declarations"][0]["default"],
            json!({"kind": "scalar", "value": REDACTED_VALUE})
        );
        assert!(!redacted.to_string().contains("module-secret"));
    }

    #[test]
    fn safe_exported_module_default_remains_available() {
        let payload = json!({
            "exported_modules": [{
                "declarations": [{
                    "path": "services.example.packageName",
                    "has_default": true,
                    "default": {"kind": "scalar", "value": "postgresql_16"}
                }]
            }]
        });
        let redacted = redact_flake_output(&payload);
        assert_eq!(
            redacted["exported_modules"][0]["declarations"][0]["default"],
            json!({"kind": "scalar", "value": "postgresql_16"})
        );
    }

    #[test]
    fn module_carrier_path_is_normalized_without_touching_declaration_paths() {
        let payload = json!({
            "exported_modules": [{
                "source_path": "./modules\\base.nix",
                "declarations": [{
                    "path": "services.example.enable",
                    "has_default": false,
                    "source_paths": ["/nix/store/declaration-source.nix"]
                }]
            }]
        });
        let redacted = redact_flake_output(&payload);
        assert_eq!(
            redacted["exported_modules"][0]["source_path"],
            "modules/base.nix"
        );
        assert_eq!(
            redacted["exported_modules"][0]["declarations"][0]["source_paths"][0],
            "/nix/store/declaration-source.nix"
        );
    }

    #[test]
    fn typed_values_preserve_safe_strings_and_remove_secret_keys() {
        let value = json!({
            "kind": "attribute_set",
            "value": {
                "GITHUB_PAT": "github-secret",
                "displayName": "postgresql-client",
                "enabled": true,
                "retries": 3,
                "nested": ["nested-secret", false]
            }
        });

        let redacted = redact_typed_value(&value);
        assert_eq!(redacted["kind"], "attribute_set");
        assert_eq!(redacted["value"]["enabled"], true);
        assert_eq!(redacted["value"]["retries"], 3);
        assert!(redacted["value"].get("GITHUB_PAT").is_none());
        assert_eq!(redacted["value"]["displayName"], "postgresql-client");
        assert_eq!(redacted["value"]["nested"][0], "nested-secret");
        let encoded = redacted.to_string();
        assert!(!encoded.contains("github-secret"));
        assert!(!encoded.contains("GITHUB_PAT"));
        assert!(encoded.contains("postgresql-client"));
        assert!(encoded.contains("nested-secret"));
    }

    #[test]
    fn sensitive_paths_and_token_patterns_are_redacted_deterministically() {
        let sensitive =
            redact_option_value("services.example.githubPat", &json!("ordinary-secret"));
        assert_eq!(sensitive, REDACTED_VALUE);

        let input = "safe release-note github_pat_11AA22bb33CC44dd55EE66ff77GG88hh eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxMjM0NTY3ODkwIn0.c2lnbmF0dXJlMTIzNDU2Nzg5MA glpat-AbCdEf0123456789AbCd https://user:pass@example.test/repo?token=secret";
        let redacted = redact_text(input);
        assert!(redacted.contains("safe release-note"));
        for secret in [
            "github_pat_",
            "eyJhbGci",
            "glpat-",
            "user:pass",
            "token=secret",
        ] {
            assert!(!redacted.contains(secret));
        }
    }

    #[test]
    fn sensitive_context_redacts_every_scalar_type_and_nested_leaf() {
        for value in [json!(null), json!(false), json!(42), json!("secret")] {
            assert_eq!(
                redact_option_value("services.example.apiToken", &value),
                REDACTED_VALUE
            );
        }

        assert_eq!(
            redact_option_value(
                "services.example.apiToken",
                &json!({"nested": [null, true, 73, "secret"]}),
            ),
            json!({"nested": [REDACTED_VALUE, REDACTED_VALUE, REDACTED_VALUE, REDACTED_VALUE]})
        );
    }
}
