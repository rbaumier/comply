//! i18n-json-valid-message-syntax backend — validate ICU MessageFormat
//! syntax in every string value of a JSON translation file.
//!
//! Strategy:
//! 1. Gate on path: only JSON files under `locales/`, `i18n/`,
//!    `translations/`, or with a `*.locale.json` / `*.i18n.json` suffix.
//! 2. Parse the file with `serde_json`. A parse error produces exactly
//!    one diagnostic at the reported line — we don't try to lint inside
//!    broken JSON.
//! 3. Recursively walk every string value and validate its ICU syntax.
//!    We locate the string's line in the source via a small substring
//!    scan (serde_json discards positions).
//!
//! The ICU validator is intentionally conservative: it flags obvious
//! structural errors (unbalanced braces, `plural`/`select` without an
//! `other` branch, empty placeholders) while accepting everything else.
//! False positives here are worse than false negatives — a brittle
//! validator discourages rule adoption.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};
use serde_json::Value;

const RULE_ID: &str = "i18n-json-valid-message-syntax";

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        if !is_i18n_file(ctx.path) {
            return Vec::new();
        }

        let mut diagnostics = Vec::new();

        let value: Value = match serde_json::from_str(ctx.source) {
            Ok(v) => v,
            Err(e) => {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: e.line().max(1),
                    column: e.column().max(1),
                    rule_id: RULE_ID.into(),
                    message: format!("Translation JSON is invalid: {e}"),
                    severity: Severity::Warning,
                    span: None,
                });
                return diagnostics;
            }
        };

        let lines: Vec<&str> = ctx.source.lines().collect();
        walk_value(&value, &mut diagnostics, ctx.path, &lines);
        diagnostics
    }
}

/// True if this JSON file is likely a translation bundle.
fn is_i18n_file(path: &std::path::Path) -> bool {
    let path_str = path.to_string_lossy().replace('\\', "/");
    let lower = path_str.to_ascii_lowercase();

    if lower.ends_with(".locale.json") || lower.ends_with(".i18n.json") {
        return true;
    }

    // Path segment heuristics: locales/, i18n/, translations/, lang/, langs/
    for segment in ["/locales/", "/i18n/", "/translations/", "/lang/", "/langs/"] {
        if lower.contains(segment) {
            return true;
        }
    }

    false
}

/// Walk every string leaf of the JSON tree, validating ICU syntax.
fn walk_value(
    value: &Value,
    diagnostics: &mut Vec<Diagnostic>,
    path: &std::path::Path,
    lines: &[&str],
) {
    match value {
        Value::String(s) => {
            if let Some(err) = validate_icu(s) {
                let line = locate_string_line(s, lines).unwrap_or(1);
                diagnostics.push(Diagnostic {
                    path: path.to_path_buf(),
                    line,
                    column: 1,
                    rule_id: RULE_ID.into(),
                    message: format!("Invalid ICU MessageFormat: {err}"),
                    severity: Severity::Warning,
                    span: None,
                });
            }
        }
        Value::Array(items) => {
            for item in items {
                walk_value(item, diagnostics, path, lines);
            }
        }
        Value::Object(map) => {
            for (_, v) in map {
                walk_value(v, diagnostics, path, lines);
            }
        }
        _ => {}
    }
}

/// Find the 1-indexed line where `needle` (a JSON string value) first
/// appears in the source. serde_json strips line info, so we do a naive
/// substring scan over the source lines. Used only for diagnostic
/// positioning — an approximate answer is fine.
fn locate_string_line(needle: &str, lines: &[&str]) -> Option<usize> {
    if needle.is_empty() {
        return None;
    }
    // Escape the first 40 chars to avoid matching across quote boundaries.
    let slice: String = needle.chars().take(40).collect();
    let escaped = slice
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n");
    for (idx, line) in lines.iter().enumerate() {
        if line.contains(&escaped) {
            return Some(idx + 1);
        }
    }
    None
}

/// Validate a single ICU MessageFormat string. Returns `Some(error)` on
/// the first structural problem, or `None` if the message is well-formed.
///
/// Covered failures:
/// - Unbalanced `{` / `}`.
/// - Empty placeholder `{}`.
/// - `plural` / `select` / `selectordinal` without an `other` branch.
/// - Branch bodies with unmatched braces.
fn validate_icu(message: &str) -> Option<String> {
    let bytes = message.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if b == b'\'' {
            // ICU apostrophe escaping: `''` literal, `'{...'` quoted literal.
            // We just skip until the closing apostrophe to avoid misreading
            // braces inside quoted literals.
            if let Some(next) = find_byte(bytes, b'\'', i + 1) {
                i = next + 1;
                continue;
            }
            // Unclosed apostrophe group — not strictly invalid in ICU
            // (apostrophe may terminate at end of string), so stop scanning.
            break;
        }
        if b == b'}' {
            return Some(format!(
                "unexpected `}}` at position {i} (no matching `{{`)"
            ));
        }
        if b == b'{' {
            let end = match find_matching_brace(bytes, i) {
                Ok(n) => n,
                Err(e) => return Some(e),
            };
            let inner = &message[i + 1..end];
            if let Some(err) = validate_placeholder(inner) {
                return Some(err);
            }
            i = end + 1;
            continue;
        }
        i += 1;
    }
    None
}

/// Given `bytes` and the index of an opening `{`, return the index of the
/// matching `}`. Returns None (as a diagnostic string) if unbalanced.
fn find_matching_brace(bytes: &[u8], open: usize) -> Result<usize, String> {
    let mut depth: i32 = 0;
    let mut i = open;
    while i < bytes.len() {
        match bytes[i] {
            b'\'' => {
                if let Some(next) = find_byte(bytes, b'\'', i + 1) {
                    i = next + 1;
                    continue;
                }
                break;
            }
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return Ok(i);
                }
            }
            _ => {}
        }
        i += 1;
    }
    Err(format!("unbalanced `{{` at position {open}"))
}

fn find_byte(bytes: &[u8], target: u8, from: usize) -> Option<usize> {
    bytes[from..].iter().position(|&b| b == target).map(|p| p + from)
}

/// Validate the interior of a placeholder `{...}`. Five forms accepted:
/// 1. `{name}` — simple argument.
/// 2. `{name, number}` / `{name, date}` / `{name, time}` — typed.
/// 3. `{name, plural, one {..} other {..}}`
/// 4. `{name, select, male {..} other {..}}`
/// 5. `{name, selectordinal, one {..} other {..}}`
fn validate_placeholder(inner: &str) -> Option<String> {
    let trimmed = inner.trim();
    if trimmed.is_empty() {
        return Some("empty placeholder `{}`".into());
    }

    let parts: Vec<&str> = trimmed.splitn(3, ',').map(str::trim).collect();
    let name = parts[0];
    if !is_valid_arg_name(name) {
        return Some(format!("invalid placeholder name `{name}`"));
    }

    if parts.len() == 1 {
        return None;
    }

    let kind = parts[1];
    match kind {
        "number" | "date" | "time" | "ordinal" | "spellout" | "duration" => None,
        "plural" | "select" | "selectordinal" => {
            let body = parts.get(2).copied().unwrap_or("");
            validate_branches(kind, body)
        }
        other if other.is_empty() => {
            Some(format!("missing format type after `{name},`"))
        }
        other => Some(format!(
            "unknown ICU format type `{other}` (expected one of: number, date, time, plural, select, selectordinal)"
        )),
    }
}

/// Validate that a `plural` / `select` / `selectordinal` body has at
/// least one branch and contains an `other` branch.
fn validate_branches(kind: &str, body: &str) -> Option<String> {
    let bytes = body.as_bytes();
    let mut i = 0;
    let mut branches: Vec<String> = Vec::new();
    while i < bytes.len() {
        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        if i >= bytes.len() {
            break;
        }
        // Parse branch key: =N or an identifier.
        let start = i;
        while i < bytes.len() && !bytes[i].is_ascii_whitespace() && bytes[i] != b'{' {
            i += 1;
        }
        if start == i {
            return Some(format!("`{kind}` body has a stray token"));
        }
        let key = &body[start..i];
        // Skip whitespace before `{`.
        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        if i >= bytes.len() || bytes[i] != b'{' {
            return Some(format!(
                "`{kind}` branch `{key}` is missing its `{{...}}` body"
            ));
        }
        let close = match find_matching_brace(bytes, i) {
            Ok(n) => n,
            Err(e) => return Some(e),
        };
        branches.push(key.to_string());
        i = close + 1;
    }

    if branches.is_empty() {
        return Some(format!("`{kind}` needs at least one branch"));
    }
    if !branches.iter().any(|b| b == "other") {
        return Some(format!(
            "`{kind}` is missing the required `other` branch"
        ));
    }
    None
}

fn is_valid_arg_name(name: &str) -> bool {
    if name.is_empty() {
        return false;
    }
    let mut chars = name.chars();
    let first = chars.next().unwrap();
    if !(first.is_ascii_alphabetic() || first == '_') {
        return false;
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::backend::CheckCtx;
    use std::path::Path;

    fn run(path: &str, source: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new(path), source))
    }

    #[test]
    fn ignores_non_i18n_json() {
        let src = r#"{ "msg": "bad {plural, count}" }"#;
        assert!(run("some/config.json", src).is_empty());
    }

    #[test]
    fn flags_unbalanced_braces_in_locales_dir() {
        let src = r#"{ "msg": "Hello {name" }"#;
        let diags = run("locales/en.json", src);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("unbalanced"));
    }

    #[test]
    fn flags_missing_other_branch_in_plural() {
        let src = r#"{ "msg": "{count, plural, one {#} two {#}}" }"#;
        let diags = run("i18n/fr.json", src);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("other"));
    }

    #[test]
    fn accepts_valid_plural() {
        let src = r#"{ "msg": "{count, plural, one {# item} other {# items}}" }"#;
        assert!(run("locales/en.json", src).is_empty());
    }

    #[test]
    fn accepts_valid_select() {
        let src = r#"{ "msg": "{gender, select, male {he} female {she} other {they}}" }"#;
        assert!(run("translations/en.json", src).is_empty());
    }

    #[test]
    fn flags_unknown_format_type() {
        let src = r#"{ "msg": "{count, unknownType}" }"#;
        let diags = run("locales/en.json", src);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("unknown ICU format type"));
    }

    #[test]
    fn flags_empty_placeholder() {
        let src = r#"{ "msg": "Hello {}" }"#;
        let diags = run("locales/en.json", src);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("empty placeholder"));
    }

    #[test]
    fn accepts_simple_placeholder() {
        let src = r#"{ "msg": "Hello {name}!" }"#;
        assert!(run("locales/en.json", src).is_empty());
    }

    #[test]
    fn accepts_nested_objects() {
        let src = r#"{
  "page": {
    "title": "Hello {name}",
    "count": "{n, plural, one {one} other {many}}"
  }
}"#;
        assert!(run("i18n/en.json", src).is_empty());
    }

    #[test]
    fn flags_nested_invalid() {
        let src = r#"{
  "page": {
    "count": "{n, plural, one {one}}"
  }
}"#;
        let diags = run("i18n/en.json", src);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("other"));
    }

    #[test]
    fn reports_parse_error_once() {
        let src = r#"{ "msg": "unterminated "#;
        let diags = run("locales/en.json", src);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("invalid"));
    }

    #[test]
    fn recognises_dot_locale_suffix() {
        let src = r#"{ "msg": "Hello {" }"#;
        let diags = run("src/messages/en.locale.json", src);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn recognises_dot_i18n_suffix() {
        let src = r#"{ "msg": "Hello {" }"#;
        let diags = run("src/messages/en.i18n.json", src);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn apostrophe_escapes_brace() {
        // In ICU, `'{'` is a literal `{`. We should not treat it as a placeholder.
        let src = r#"{ "msg": "Show '{'raw'}' text" }"#;
        assert!(run("locales/en.json", src).is_empty());
    }

    #[test]
    fn accepts_plural_with_exact_keys() {
        let src = r#"{ "msg": "{count, plural, =0 {none} one {#} other {#}}" }"#;
        assert!(run("locales/en.json", src).is_empty());
    }
}
