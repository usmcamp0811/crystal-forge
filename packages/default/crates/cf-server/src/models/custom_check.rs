//! Defines custom-check configuration validation and binding normalization.
//!
//! API writes and CF-XCCDF native imports use this module so persisted policy
//! configuration and imported executable projections have one contract.

use std::collections::HashSet;

use serde_json::Value;

use super::deployment_policies::{is_reserved_policy_result_field, validate_custom_eval_syntax};

/// Selects the executable binding accepted while validating expressions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExpressionBinding {
    /// Accepts current and historical references and returns current references.
    Compatible,
    /// Accepts only historical `cfg.config` references and returns current references.
    Legacy,
    /// Accepts only current `config` references.
    Current,
}

/// Validates and normalizes one custom-check configuration.
///
/// Non-empty `rules` take precedence over a top-level `expression`. A
/// top-level expression takes precedence over a missing or empty `rules`
/// array. An absent expression with `mode = "all"` and an empty `rules` array
/// is the explicit no-enforcement form. Unknown fields are preserved.
///
/// # Errors
///
/// Returns an error for an invalid object shape, mode, strict value, rule,
/// result field name, expression binding, or Nix syntax. `mode = "any"` with
/// an effective empty rule set is invalid.
pub fn validate_and_normalize_config(
    config: &Value,
    binding: ExpressionBinding,
    validate_nix_syntax: bool,
) -> Result<Value, String> {
    validate_config_inner(config, binding, validate_nix_syntax, true)
}

/// Validates one custom-check configuration without rewriting its expressions.
///
/// # Errors
///
/// Returns the same errors as [`validate_and_normalize_config`].
pub fn validate_config(
    config: &Value,
    binding: ExpressionBinding,
    validate_nix_syntax: bool,
) -> Result<(), String> {
    validate_config_inner(config, binding, validate_nix_syntax, false).map(drop)
}

fn validate_config_inner(
    config: &Value,
    binding: ExpressionBinding,
    validate_nix_syntax: bool,
    normalize: bool,
) -> Result<Value, String> {
    let object = config
        .as_object()
        .ok_or_else(|| "Policy config must be a JSON object".to_string())?;
    let rules = match object.get("rules") {
        None => None,
        Some(Value::Array(rules)) => Some(rules),
        Some(_) => return Err("config.rules must be an array when provided".into()),
    };
    let has_rules = rules.is_some_and(|rules| !rules.is_empty());
    let expression = if has_rules {
        None
    } else {
        match object.get("expression") {
            Some(Value::String(expression)) if !expression.trim().is_empty() => {
                Some(expression.trim())
            }
            Some(Value::String(_)) | None => None,
            Some(_) => return Err("config.expression must be a string when provided".into()),
        }
    };
    let mode = if has_rules || expression.is_none() {
        match object.get("mode") {
            None => "all",
            Some(Value::String(mode)) if matches!(mode.as_str(), "all" | "any") => mode.as_str(),
            Some(Value::String(_)) => {
                return Err("config.mode must be \"all\" or \"any\"".into());
            }
            Some(_) => return Err("config.mode must be a string (\"all\" or \"any\")".into()),
        }
    } else {
        // Runtime always aggregates the single-expression shape as `all`.
        "all"
    };
    if !has_rules && expression.is_none() && rules.is_none() {
        return Err("custom_check policy requires config.expression or config.rules[]".into());
    }
    if !has_rules && expression.is_none() && mode == "any" {
        return Err("config.mode \"any\" requires at least one rule".into());
    }

    let mut normalized = config.clone();
    let mut expressions = Vec::new();
    if has_rules {
        let rules =
            rules.ok_or_else(|| "config.rules must be an array when provided".to_string())?;
        let mut seen = HashSet::with_capacity(rules.len());
        let mut normalized_expressions = Vec::with_capacity(rules.len());
        for (index, rule) in rules.iter().enumerate() {
            let rule = rule
                .as_object()
                .ok_or_else(|| format!("config.rules[{index}] must be an object"))?;
            let expression = rule
                .get("expression")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|expression| !expression.is_empty())
                .ok_or_else(|| {
                    format!("config.rules[{index}].expression must be a non-empty string")
                })?;
            let expression = normalize_expression_for_binding(expression, binding)?;
            expressions.push(expression.clone());
            normalized_expressions.push(expression);

            let field_name = rule
                .get("field_name")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|field_name| !field_name.is_empty())
                .ok_or_else(|| {
                    format!("config.rules[{index}].field_name must be a non-empty string")
                })?;
            if !seen.insert(field_name.to_owned()) {
                return Err(format!(
                    "config.rules[{index}].field_name duplicates existing field_name '{field_name}'"
                ));
            }
            if is_reserved_policy_result_field(field_name) {
                return Err(format!(
                    "config.rules[{index}].field_name '{field_name}' is reserved for built-in evaluator metadata"
                ));
            }
            if let Some(strict) = rule.get("strict")
                && !strict.is_boolean()
            {
                return Err(format!(
                    "config.rules[{index}].strict must be a boolean when provided"
                ));
            }
        }
        if normalize
            && let Some(normalized_rules) =
                normalized.get_mut("rules").and_then(Value::as_array_mut)
        {
            for (rule, expression) in normalized_rules.iter_mut().zip(normalized_expressions) {
                let rule = rule
                    .as_object_mut()
                    .ok_or_else(|| "validated rule did not remain an object".to_string())?;
                rule.insert("expression".into(), Value::String(expression));
            }
        }
    } else if let Some(expression) = expression {
        if let Some(strict) = object.get("strict")
            && !strict.is_boolean()
        {
            return Err("config.strict must be a boolean when provided".to_string());
        }
        let expression = normalize_expression_for_binding(expression, binding)?;
        expressions.push(expression.clone());
        if normalize {
            let normalized_object = normalized
                .as_object_mut()
                .ok_or_else(|| "validated config did not remain an object".to_string())?;
            normalized_object.insert("expression".into(), Value::String(expression));
        }
        if let Some(field_name) = object.get("field_name") {
            let field_name = field_name
                .as_str()
                .map(str::trim)
                .filter(|field_name| !field_name.is_empty())
                .ok_or_else(|| "config.field_name must be a non-empty string".to_string())?;
            if is_reserved_policy_result_field(field_name) {
                return Err(format!(
                    "config.field_name '{field_name}' is reserved for built-in evaluator metadata"
                ));
            }
        }
    }

    if validate_nix_syntax && !expressions.is_empty() {
        validate_custom_eval_syntax(&expressions.iter().map(String::as_str).collect::<Vec<_>>())
            .map_err(|error| format!("custom_check expression is invalid: {error}"))?;
    }
    Ok(normalized)
}

fn normalize_expression_for_binding(
    expression: &str,
    binding: ExpressionBinding,
) -> Result<String, String> {
    let (normalized, has_legacy) = normalize_expression(expression);
    let has_current = has_current_reference(expression);
    let has_unsupported_legacy = has_executable_identifier(&normalized, "cfg");
    match binding {
        ExpressionBinding::Legacy if has_current => {
            return Err("expression uses the V2 config binding in a V1 context".into());
        }
        ExpressionBinding::Current if has_legacy => {
            return Err("expression uses the historical cfg.config binding".into());
        }
        _ => {}
    }
    if has_unsupported_legacy {
        return Err("expression uses unsupported executable access through the cfg binding".into());
    }
    Ok(normalized)
}

/// Normalizes executable historical references without changing literals or comments.
pub fn normalize_expression(expression: &str) -> (String, bool) {
    let mut normalized = expression.to_owned();
    let mut changed = false;
    for source in ["cfg.${\"config\"}", "cfg.\"config\"", "cfg.config"] {
        let (next, matched) = rewrite_token(&normalized, source, "config");
        normalized = next;
        changed |= matched;
    }
    (normalized, changed)
}

/// Reports whether executable code uses the current `config` binding.
pub fn has_current_reference(expression: &str) -> bool {
    has_executable_identifier(&mask_legacy_references(expression), "config")
}

/// Reports whether executable code uses the historical `cfg.config` binding.
pub fn has_legacy_reference(expression: &str) -> bool {
    normalize_expression(expression).1
}

fn mask_legacy_references(expression: &str) -> String {
    let mut masked = expression.to_owned();
    for source in ["cfg.${\"config\"}", "cfg.\"config\"", "cfg.config"] {
        masked = rewrite_token(&masked, source, "__cf_legacy_binding").0;
    }
    masked
}

fn has_executable_identifier(expression: &str, identifier: &str) -> bool {
    rewrite_token(expression, identifier, "__cf_binding_reference").1
}

fn rewrite_token(expression: &str, source: &str, replacement: &str) -> (String, bool) {
    let chars = expression.chars().collect::<Vec<_>>();
    let source_chars = source.chars().collect::<Vec<_>>();
    let mut output = String::with_capacity(expression.len());
    let mut index = 0;
    let mut state = LexicalState::Normal;
    let mut block_comment_depth = 0usize;
    while index < chars.len() {
        match state {
            LexicalState::Normal => {
                if chars[index] == '"' {
                    state = LexicalState::DoubleQuoted;
                    output.push(chars[index]);
                    index += 1;
                } else if chars[index] == '#' {
                    state = LexicalState::LineComment;
                    output.push(chars[index]);
                    index += 1;
                } else if chars[index] == '/' && chars.get(index + 1) == Some(&'*') {
                    state = LexicalState::BlockComment;
                    block_comment_depth = 1;
                    output.push_str("/*");
                    index += 2;
                } else if chars[index] == '\'' && chars.get(index + 1) == Some(&'\'') {
                    state = LexicalState::IndentedString;
                    output.push_str("''");
                    index += 2;
                } else if is_search_path_start(&chars, index) {
                    state = LexicalState::SearchPath;
                    output.push(chars[index]);
                    index += 1;
                } else if is_uri_literal_start(&chars, index) {
                    state = LexicalState::UriLiteral;
                    output.push(chars[index]);
                    index += 1;
                } else if is_path_literal_start(&chars, index) {
                    state = LexicalState::PathLiteral;
                    output.push(chars[index]);
                    index += 1;
                } else if index + source_chars.len() <= chars.len()
                    && chars[index..index + source_chars.len()] == source_chars
                    && is_token_start(&chars, index)
                    && is_identifier_end(&chars, index + source_chars.len())
                    && !is_attribute_name(&chars, index + source_chars.len())
                {
                    output.push_str(replacement);
                    index += source_chars.len();
                } else {
                    output.push(chars[index]);
                    index += 1;
                }
            }
            LexicalState::DoubleQuoted => {
                let character = chars[index];
                if character == '\\' {
                    output.push(character);
                    index += 1;
                    if let Some(escaped) = chars.get(index) {
                        output.push(*escaped);
                        index += 1;
                    }
                } else if character == '$' && chars.get(index + 1) == Some(&'{') {
                    let start = index + 2;
                    if let Some(end) = interpolation_end(&chars, start) {
                        let inner = chars[start..end].iter().collect::<String>();
                        output.push_str("${");
                        output.push_str(&rewrite_token(&inner, source, replacement).0);
                        output.push('}');
                        index = end + 1;
                    } else {
                        output.push(character);
                        index += 1;
                    }
                } else if character == '"' {
                    output.push(character);
                    index += 1;
                    state = LexicalState::Normal;
                } else {
                    output.push(character);
                    index += 1;
                }
            }
            LexicalState::IndentedString => {
                if chars[index] == '\'' && chars.get(index + 1) == Some(&'\'') {
                    if chars.get(index + 2) == Some(&'\\') {
                        output.push_str("''\\");
                        index += 3;
                        if let Some(escaped) = chars.get(index) {
                            output.push(*escaped);
                            index += 1;
                        }
                        continue;
                    }
                    if chars
                        .get(index + 2)
                        .is_some_and(|next| matches!(next, '$' | '\''))
                    {
                        output.push_str("''");
                        index += 2;
                        if let Some(escaped) = chars.get(index) {
                            output.push(*escaped);
                            index += 1;
                        }
                        continue;
                    }
                    output.push_str("''");
                    index += 2;
                    state = LexicalState::Normal;
                } else if chars[index] == '$' && chars.get(index + 1) == Some(&'{') {
                    let start = index + 2;
                    if let Some(end) = interpolation_end(&chars, start) {
                        let inner = chars[start..end].iter().collect::<String>();
                        output.push_str("${");
                        output.push_str(&rewrite_token(&inner, source, replacement).0);
                        output.push('}');
                        index = end + 1;
                    } else {
                        output.push(chars[index]);
                        index += 1;
                    }
                } else {
                    output.push(chars[index]);
                    index += 1;
                }
            }
            LexicalState::LineComment => {
                let character = chars[index];
                output.push(character);
                index += 1;
                if character == '\n' {
                    state = LexicalState::Normal;
                }
            }
            LexicalState::BlockComment => {
                if chars[index] == '/' && chars.get(index + 1) == Some(&'*') {
                    block_comment_depth += 1;
                    output.push_str("/*");
                    index += 2;
                } else if chars[index] == '*' && chars.get(index + 1) == Some(&'/') {
                    block_comment_depth -= 1;
                    output.push_str("*/");
                    index += 2;
                    if block_comment_depth == 0 {
                        state = LexicalState::Normal;
                    }
                } else {
                    output.push(chars[index]);
                    index += 1;
                }
            }
            LexicalState::PathLiteral | LexicalState::UriLiteral => {
                let is_literal_character = if matches!(state, LexicalState::PathLiteral) {
                    is_path_literal_character(chars[index])
                } else {
                    is_uri_literal_character(chars[index])
                };
                if !is_literal_character
                    && !(chars[index] == '$' && chars.get(index + 1) == Some(&'{'))
                {
                    state = LexicalState::Normal;
                } else if chars[index] == '$' && chars.get(index + 1) == Some(&'{') {
                    let start = index + 2;
                    if let Some(end) = interpolation_end(&chars, start) {
                        let inner = chars[start..end].iter().collect::<String>();
                        output.push_str("${");
                        output.push_str(&rewrite_token(&inner, source, replacement).0);
                        output.push('}');
                        index = end + 1;
                    } else {
                        output.push(chars[index]);
                        index += 1;
                    }
                } else {
                    output.push(chars[index]);
                    index += 1;
                }
            }
            LexicalState::SearchPath => {
                output.push(chars[index]);
                index += 1;
                if chars[index - 1] == '>' {
                    state = LexicalState::Normal;
                }
            }
        }
    }
    let output = rewrite_string_interpolations(&output, source, replacement);
    let changed = output != expression;
    (output, changed)
}

fn is_token_start(chars: &[char], index: usize) -> bool {
    index == 0 || (!is_nix_identifier_char(chars[index - 1]) && chars[index - 1] != '.')
}

fn is_identifier_end(chars: &[char], index: usize) -> bool {
    chars
        .get(index)
        .is_none_or(|character| !is_nix_identifier_char(*character))
}

fn is_attribute_name(chars: &[char], mut index: usize) -> bool {
    while chars
        .get(index)
        .is_some_and(|character| character.is_whitespace())
    {
        index += 1;
    }
    chars.get(index) == Some(&'=') && chars.get(index + 1) != Some(&'=')
}

fn is_path_literal_start(chars: &[char], index: usize) -> bool {
    let starts_absolute = chars.get(index) == Some(&'/')
        && chars
            .get(index + 1)
            .is_some_and(|next| !next.is_whitespace() && !matches!(next, '/' | '*'));
    let starts_relative = chars.get(index..index + 2) == Some(&['.', '/'])
        || chars.get(index..index + 3) == Some(&['.', '.', '/'])
        || is_bare_relative_path_start(chars, index);
    (starts_absolute || starts_relative) && is_literal_start_context(chars, index)
}

fn is_bare_relative_path_start(chars: &[char], index: usize) -> bool {
    let mut cursor = index;
    while chars.get(cursor).is_some_and(|character| {
        character.is_ascii_alphanumeric() || matches!(character, '+' | '-' | '.' | '_')
    }) {
        cursor += 1;
    }
    cursor > index
        && chars.get(cursor) == Some(&'/')
        && chars
            .get(cursor + 1)
            .is_some_and(|next| !next.is_whitespace() && !matches!(next, '/' | '*'))
}

fn is_search_path_start(chars: &[char], index: usize) -> bool {
    chars.get(index) == Some(&'<')
        && is_literal_start_context(chars, index)
        && chars[index + 1..]
            .iter()
            .take_while(|character| !character.is_whitespace())
            .any(|character| *character == '>')
}

fn is_uri_literal_start(chars: &[char], index: usize) -> bool {
    if !is_literal_start_context(chars, index)
        || !chars
            .get(index)
            .is_some_and(|character| character.is_ascii_alphabetic())
    {
        return false;
    }
    let mut cursor = index + 1;
    while chars.get(cursor).is_some_and(|character| {
        character.is_ascii_alphanumeric() || matches!(character, '+' | '-' | '.')
    }) {
        cursor += 1;
    }
    chars.get(cursor) == Some(&':')
        && chars
            .get(cursor + 1)
            .is_some_and(|character| is_uri_literal_character(*character))
}

fn is_literal_start_context(chars: &[char], index: usize) -> bool {
    index == 0
        || chars[index - 1].is_whitespace()
        || matches!(
            chars[index - 1],
            '=' | '(' | '[' | '{' | ':' | ';' | ',' | '!' | '?'
        )
}

fn is_path_literal_character(character: char) -> bool {
    character.is_ascii_alphanumeric() || matches!(character, '/' | '+' | '-' | '.' | '_')
}

fn is_uri_literal_character(character: char) -> bool {
    character.is_ascii_alphanumeric()
        || matches!(
            character,
            '%' | '/'
                | '?'
                | ':'
                | '@'
                | '&'
                | '='
                | '+'
                | '$'
                | ','
                | '_'
                | '.'
                | '!'
                | '~'
                | '*'
                | '\''
                | '-'
        )
}

fn rewrite_string_interpolations(expression: &str, source: &str, replacement: &str) -> String {
    let chars = expression.chars().collect::<Vec<_>>();
    let mut output = String::with_capacity(expression.len());
    let mut index = 0;
    let mut string_kind = None::<bool>;
    let mut line_comment = false;
    let mut block_comment_depth = 0usize;
    while index < chars.len() {
        if string_kind.is_none() {
            if line_comment {
                let character = chars[index];
                output.push(character);
                index += 1;
                if character == '\n' {
                    line_comment = false;
                }
                continue;
            }
            if block_comment_depth > 0 {
                if chars[index] == '/' && chars.get(index + 1) == Some(&'*') {
                    block_comment_depth += 1;
                    output.push_str("/*");
                    index += 2;
                } else if chars[index] == '*' && chars.get(index + 1) == Some(&'/') {
                    block_comment_depth -= 1;
                    output.push_str("*/");
                    index += 2;
                } else {
                    output.push(chars[index]);
                    index += 1;
                }
                continue;
            }
            if chars[index] == '#' {
                line_comment = true;
            } else if chars[index] == '/' && chars.get(index + 1) == Some(&'*') {
                block_comment_depth = 1;
                output.push_str("/*");
                index += 2;
                continue;
            } else if chars[index] == '"' {
                string_kind = Some(false);
            } else if chars[index] == '\'' && chars.get(index + 1) == Some(&'\'') {
                string_kind = Some(true);
                output.push('\'');
                index += 1;
            }
            output.push(chars[index]);
            index += 1;
            continue;
        }
        let indented = string_kind == Some(true);
        if !indented && chars[index] == '\\' {
            output.push(chars[index]);
            if let Some(next) = chars.get(index + 1) {
                output.push(*next);
                index += 2;
            } else {
                index += 1;
            }
            continue;
        }
        if indented && chars[index] == '\'' && chars.get(index + 1) == Some(&'\'') {
            if chars.get(index + 2) == Some(&'\\') {
                output.push_str("''\\");
                index += 3;
                if let Some(escaped) = chars.get(index) {
                    output.push(*escaped);
                    index += 1;
                }
                continue;
            }
            if chars
                .get(index + 2)
                .is_some_and(|next| matches!(next, '$' | '\''))
            {
                output.push_str("''");
                index += 2;
                if let Some(escaped) = chars.get(index) {
                    output.push(*escaped);
                    index += 1;
                }
                continue;
            }
            output.push_str("''");
            index += 2;
            string_kind = None;
            continue;
        }
        if chars[index] == '$' && chars.get(index + 1) == Some(&'{') {
            let start = index + 2;
            if let Some(end) = interpolation_end(&chars, start) {
                let inner = chars[start..end].iter().collect::<String>();
                output.push_str("${");
                output.push_str(&rewrite_token(&inner, source, replacement).0);
                output.push('}');
                index = end + 1;
                continue;
            }
        }
        if !indented && chars[index] == '"' {
            string_kind = None;
        }
        output.push(chars[index]);
        index += 1;
    }
    output
}

fn interpolation_end(chars: &[char], mut index: usize) -> Option<usize> {
    let mut depth = 1usize;
    let mut state = LexicalState::Normal;
    let mut block_comment_depth = 0usize;
    while index < chars.len() {
        match state {
            LexicalState::Normal => match chars[index] {
                '"' => {
                    state = LexicalState::DoubleQuoted;
                    index += 1;
                }
                '#' => {
                    state = LexicalState::LineComment;
                    index += 1;
                }
                '/' if chars.get(index + 1) == Some(&'*') => {
                    state = LexicalState::BlockComment;
                    block_comment_depth = 1;
                    index += 2;
                }
                '\'' if chars.get(index + 1) == Some(&'\'') => {
                    state = LexicalState::IndentedString;
                    index += 2;
                }
                '<' if is_search_path_start(chars, index) => {
                    state = LexicalState::SearchPath;
                    index += 1;
                }
                _ if is_uri_literal_start(chars, index) => {
                    state = LexicalState::UriLiteral;
                    index += 1;
                }
                _ if is_path_literal_start(chars, index) => {
                    state = LexicalState::PathLiteral;
                    index += 1;
                }
                '{' => {
                    depth += 1;
                    index += 1;
                }
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        return Some(index);
                    }
                    index += 1;
                }
                _ => index += 1,
            },
            LexicalState::DoubleQuoted => {
                if chars[index] == '\\' {
                    index += 2;
                } else if chars[index] == '$' && chars.get(index + 1) == Some(&'{') {
                    index = interpolation_end(chars, index + 2)? + 1;
                } else if chars[index] == '"' {
                    state = LexicalState::Normal;
                    index += 1;
                } else {
                    index += 1;
                }
            }
            LexicalState::IndentedString => {
                if chars[index] == '\'' && chars.get(index + 1) == Some(&'\'') {
                    if chars.get(index + 2) == Some(&'\\') {
                        index += 3;
                        if index < chars.len() {
                            index += 1;
                        }
                    } else if chars
                        .get(index + 2)
                        .is_some_and(|next| matches!(next, '$' | '\''))
                    {
                        index += 3;
                    } else {
                        state = LexicalState::Normal;
                        index += 2;
                    }
                } else if chars[index] == '$' && chars.get(index + 1) == Some(&'{') {
                    index = interpolation_end(chars, index + 2)? + 1;
                } else {
                    index += 1;
                }
            }
            LexicalState::LineComment => {
                if chars[index] == '\n' {
                    state = LexicalState::Normal;
                }
                index += 1;
            }
            LexicalState::BlockComment => {
                if chars[index] == '/' && chars.get(index + 1) == Some(&'*') {
                    block_comment_depth += 1;
                    index += 2;
                } else if chars[index] == '*' && chars.get(index + 1) == Some(&'/') {
                    block_comment_depth -= 1;
                    index += 2;
                    if block_comment_depth == 0 {
                        state = LexicalState::Normal;
                    }
                } else {
                    index += 1;
                }
            }
            LexicalState::PathLiteral | LexicalState::UriLiteral => {
                let is_literal_character = if matches!(state, LexicalState::PathLiteral) {
                    is_path_literal_character(chars[index])
                } else {
                    is_uri_literal_character(chars[index])
                };
                if !is_literal_character
                    && !(chars[index] == '$' && chars.get(index + 1) == Some(&'{'))
                {
                    state = LexicalState::Normal;
                } else if chars[index] == '$' && chars.get(index + 1) == Some(&'{') {
                    index = interpolation_end(chars, index + 2)? + 1;
                } else {
                    index += 1;
                }
            }
            LexicalState::SearchPath => {
                if chars[index] == '>' {
                    state = LexicalState::Normal;
                }
                index += 1;
            }
        }
    }
    None
}

#[derive(Clone, Copy)]
enum LexicalState {
    Normal,
    DoubleQuoted,
    IndentedString,
    LineComment,
    BlockComment,
    PathLiteral,
    SearchPath,
    UriLiteral,
}

fn is_nix_identifier_char(character: char) -> bool {
    character.is_ascii_alphanumeric() || matches!(character, '_' | '-')
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn precedence_matches_runtime_and_api() {
        let expression = validate_and_normalize_config(
            &json!({"expression": "config.a", "rules": []}),
            ExpressionBinding::Current,
            false,
        )
        .unwrap();
        assert_eq!(expression["expression"], "config.a");
        assert!(
            validate_and_normalize_config(
                &json!({"mode": "all", "rules": []}),
                ExpressionBinding::Current,
                false,
            )
            .is_ok()
        );
        assert!(
            validate_and_normalize_config(
                &json!({"mode": "any", "rules": []}),
                ExpressionBinding::Current,
                false,
            )
            .is_err()
        );
        let rules = validate_and_normalize_config(
            &json!({
                "mode": "any",
                "strict": "ignored",
                "expression": {"malformed": true},
                "field_name": 42,
                "description": false,
                "rules": [{"field_name": "effective", "expression": "config.a", "strict": true}]
            }),
            ExpressionBinding::Current,
            false,
        )
        .expect("non-empty rules must ignore malformed top-level single-expression fields");
        assert_eq!(rules["rules"][0]["expression"], "config.a");
    }

    #[test]
    fn quoted_and_dynamic_selections_follow_binding_contract() {
        let source = json!({
            "rules": [
                {"field_name": "quoted", "expression": "cfg.\"config\".\"services.demo\".enable"},
                {"field_name": "static_dynamic", "expression": "cfg.${\"config\"}.${name}.enable"},
                {"field_name": "dynamic", "expression": "cfg.config.${name}.enable"}
            ]
        });
        let normalized =
            validate_and_normalize_config(&source, ExpressionBinding::Legacy, false).unwrap();
        assert_eq!(
            normalized["rules"][0]["expression"],
            "config.\"services.demo\".enable"
        );
        assert_eq!(
            normalized["rules"][1]["expression"],
            "config.${name}.enable"
        );
        assert_eq!(
            normalized["rules"][2]["expression"],
            "config.${name}.enable"
        );
        assert!(
            validate_and_normalize_config(&normalized, ExpressionBinding::Legacy, false).is_err()
        );
    }

    #[test]
    fn binding_scan_preserves_literals_comments_and_attribute_names() {
        let expression = r#"let literal = "cfg.config.value ${cfg."config".inside}";
          attrs = { cfg = "cfg.config.value"; };
          # cfg.config.comment
          block = /* cfg.config.comment */ attrs.cfg;
        in cfg.config.actual && literal != """#;
        let (normalized, changed) = normalize_expression(expression);
        assert!(changed);
        assert!(normalized.contains(r#""cfg.config.value ${config.inside}""#));
        assert!(normalized.contains("# cfg.config.comment"));
        assert!(normalized.contains("/* cfg.config.comment */ attrs.cfg"));
        assert!(normalized.contains("in config.actual"));

        let paths = r#"[
          /etc/cfg.config
          ./cfg.config
          ../cfg.config
          modules/cfg.config/default.nix
          <cfg.config/foo>
          https://example.test/cfg.config
          urn:cfg.config:value
          cfg.config.executable
        ]"#;
        let (normalized_paths, changed) = normalize_expression(paths);
        assert!(changed);
        assert!(normalized_paths.contains("/etc/cfg.config"));
        assert!(normalized_paths.contains("./cfg.config"));
        assert!(normalized_paths.contains("../cfg.config"));
        assert!(normalized_paths.contains("modules/cfg.config/default.nix"));
        assert!(normalized_paths.contains("<cfg.config/foo>"));
        assert!(normalized_paths.contains("https://example.test/cfg.config"));
        assert!(normalized_paths.contains("urn:cfg.config:value"));
        assert!(normalized_paths.contains("config.executable"));

        let (adjacent, changed) =
            normalize_expression("modules/cfg.config/default.nix==cfg.config.executable");
        assert!(changed);
        assert_eq!(
            adjacent,
            "modules/cfg.config/default.nix==config.executable"
        );

        let literals_only = json!({
            "expression": "[ modules/cfg.config/default.nix urn:cfg.config:value ]"
        });
        assert!(
            validate_and_normalize_config(&literals_only, ExpressionBinding::Current, false)
                .is_ok()
        );

        let attributes = json!({
            "expression": "let attrs = { cfg = 1; config = 2; }; in attrs.cfg == attrs.config"
        });
        assert!(
            validate_and_normalize_config(&attributes, ExpressionBinding::Compatible, false)
                .is_ok()
        );
    }

    #[test]
    fn binding_scan_ignores_interpolation_syntax_in_nested_comments() {
        let expression = r#"let
          line = 1; # "${cfg.config.line}"
          block = 2; /* outer "${cfg.config.outer}" /* nested "${cfg.config.nested}" */ cfg.config.outer_tail */
        in "${let
          # "${cfg.config.inner_line}"
          value = /* outer "${cfg.config.inner_outer}" /* nested "${cfg.config.inner_nested}" */ cfg.config.inner_tail */ cfg.config.actual;
        in value}""#;

        let (normalized, changed) = normalize_expression(expression);

        assert!(changed);
        assert!(normalized.contains(r#"# "${cfg.config.line}""#));
        assert!(normalized.contains(
            r#"/* outer "${cfg.config.outer}" /* nested "${cfg.config.nested}" */ cfg.config.outer_tail */"#
        ));
        assert!(
            normalized.contains(r#"# "${cfg.config.inner_line}""#),
            "normalized expression:\n{normalized}"
        );
        assert!(normalized.contains(
            r#"/* outer "${cfg.config.inner_outer}" /* nested "${cfg.config.inner_nested}" */ cfg.config.inner_tail */ config.actual"#
        ));
    }

    #[test]
    fn binding_scan_preserves_indented_string_interpolation_escapes() {
        let source = "''\n  ${cfg.config.actual}\n  echo ''${cfg.config.literal}\n  echo ''\\${cfg.config.literal_backslash}\n''";
        let expected = "''\n  ${config.actual}\n  echo ''${cfg.config.literal}\n  echo ''\\${cfg.config.literal_backslash}\n''";

        assert_eq!(normalize_expression(source), (expected.to_string(), true));

        let escaped_only = json!({
            "expression": "''\n  echo ''${cfg.config.literal}\n  echo ''\\${cfg.config.literal_backslash}\n''"
        });
        assert_eq!(
            validate_and_normalize_config(&escaped_only, ExpressionBinding::Current, false)
                .expect("V2 must accept literal cfg.config text in indented strings"),
            escaped_only
        );
    }

    #[test]
    fn rejects_indirect_cfg_access_in_all_supported_contexts() {
        for expression in [
            "cfg.services.demo.enable",
            "cfg.${name}.enable",
            "let inherited = cfg; in inherited.config.value",
            r#""value ${cfg.services.demo.enable}""#,
        ] {
            let config = json!({"expression": expression});
            assert!(
                validate_and_normalize_config(&config, ExpressionBinding::Compatible, false)
                    .unwrap_err()
                    .contains("unsupported executable access"),
                "accepted indirect cfg access: {expression}"
            );
            assert!(
                validate_and_normalize_config(&config, ExpressionBinding::Current, false).is_err(),
                "accepted indirect cfg access in V2: {expression}"
            );
        }
    }

    #[test]
    fn rejects_every_malformed_custom_check_shape() {
        let malformed = [
            json!(null),
            json!({}),
            json!({"expression": " "}),
            json!({"expression": false, "rules": []}),
            json!({"expression": "true", "strict": "yes"}),
            json!({"expression": "true", "field_name": ""}),
            json!({"expression": "true", "field_name": "cfAgentEnabled"}),
            json!({"rules": {}}),
            json!({"rules": [false]}),
            json!({"rules": [{"field_name": "x", "expression": ""}]}),
            json!({"rules": [{"field_name": "", "expression": "true"}]}),
            json!({"rules": [{"field_name": "x", "expression": "true", "strict": 1}]}),
            json!({"rules": [
                {"field_name": "x", "expression": "true"},
                {"field_name": "x", "expression": "false"}
            ]}),
            json!({"rules": [{"field_name": "requestedSourceRevision", "expression": "true"}]}),
        ];
        for config in malformed {
            assert!(
                validate_and_normalize_config(&config, ExpressionBinding::Compatible, false)
                    .is_err(),
                "accepted malformed config: {config}"
            );
        }
    }

    #[test]
    fn rejects_malformed_nix_syntax() {
        let error = validate_and_normalize_config(
            &json!({"expression": "config.services.["}),
            ExpressionBinding::Current,
            true,
        )
        .expect_err("the shared persistence validator must parse Nix syntax");
        assert!(error.contains("invalid Nix expression syntax"));
    }
}
