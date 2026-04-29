//! Validates ICU message format syntax in i18n JSON files.
//!
//! Applies to JSON files in:
//! - locales/, translations/, i18n/, lang/, messages/ directories
//! - Files matching locale patterns: en.json, fr-FR.json, etc.

use crate::diagnostic::{Diagnostic, Severity};
use crate::icu;
use crate::rules::backend::{CheckCtx, TextCheck};
use serde_json::Value;

#[derive(Debug)]
pub struct Check;

fn is_i18n_file(path: &std::path::Path) -> bool {
    let path_str = path.to_string_lossy();

    // Check directory patterns
    if path_str.contains("/locales/")
        || path_str.contains("/translations/")
        || path_str.contains("/i18n/")
        || path_str.contains("/lang/")
        || path_str.contains("/messages/")
    {
        return true;
    }

    // Check filename patterns (en.json, fr-FR.json, zh_CN.json)
    if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
        // 2-letter language code
        if stem.len() == 2 && stem.chars().all(|c| c.is_ascii_lowercase()) {
            return true;
        }
        // Language with region: en-US, fr-FR, zh_CN
        if (stem.len() == 5 || stem.len() == 6)
            && (stem.contains('-') || stem.contains('_'))
        {
            let parts: Vec<&str> = stem.split(['-', '_']).collect();
            if parts.len() == 2
                && parts[0].len() == 2
                && parts[0].chars().all(|c| c.is_ascii_lowercase())
            {
                return true;
            }
        }
    }

    false
}

fn validate_json_strings(
    value: &Value,
    path: &[String],
    diagnostics: &mut Vec<(String, String, usize)>,
    source: &str,
) {
    match value {
        Value::String(s) => {
            if !s.contains('{') {
                return;
            }
            // i18next uses {{var}} and $t() — not ICU syntax.
            if s.contains("{{") || s.contains("$t(") {
                return;
            }
            if let Err(e) = icu::parse(s) {
                let key_path = path.join(".");
                let line = find_line_for_key(source, path.last().map(|s| s.as_str()).unwrap_or(""));
                diagnostics.push((key_path, e.to_string(), line));
            }
        }
        Value::Object(map) => {
            for (key, val) in map {
                let mut new_path = path.to_vec();
                new_path.push(key.clone());
                validate_json_strings(val, &new_path, diagnostics, source);
            }
        }
        Value::Array(arr) => {
            for (i, val) in arr.iter().enumerate() {
                let mut new_path = path.to_vec();
                new_path.push(i.to_string());
                validate_json_strings(val, &new_path, diagnostics, source);
            }
        }
        _ => {}
    }
}

fn find_line_for_key(source: &str, key: &str) -> usize {
    let search = format!("\"{key}\"");
    for (i, line) in source.lines().enumerate() {
        if line.contains(&search) {
            return i + 1;
        }
    }
    1
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        // Only check JSON files
        if ctx.path.extension().and_then(|e| e.to_str()) != Some("json") {
            return vec![];
        }

        // Only check i18n files
        if !is_i18n_file(ctx.path) {
            return vec![];
        }

        let Ok(json) = serde_json::from_str::<Value>(ctx.source) else {
            return vec![]; // Invalid JSON is not our concern
        };

        let mut errors = Vec::new();
        validate_json_strings(&json, &[], &mut errors, ctx.source);

        errors
            .into_iter()
            .map(|(key, error, line)| Diagnostic {
                path: std::sync::Arc::clone(&ctx.path_arc),
                line,
                column: 1,
                rule_id: super::META.id.into(),
                message: format!("Invalid ICU syntax in `{key}`: {error}"),
                severity: Severity::Error,
                span: None,
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn check_json(path: &str, content: &str) -> Vec<Diagnostic> {
        let ctx = CheckCtx::for_test(Path::new(path), content);
        Check.check(&ctx)
    }

    #[test]
    fn detects_unclosed_brace() {
        let json = r#"{"greeting": "Hello {name"}"#;
        let diags = check_json("/app/locales/en.json", json);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("unclosed brace"));
    }

    #[test]
    fn detects_missing_other_in_plural() {
        let json = r#"{"count": "{n, plural, one {# item}}"}"#;
        let diags = check_json("/app/i18n/fr.json", json);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("other"));
    }

    #[test]
    fn detects_invalid_plural_keyword() {
        let json = r#"{"items": "{n, plural, single {one} other {many}}"}"#;
        let diags = check_json("/translations/en.json", json);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("invalid plural keyword"));
    }

    #[test]
    fn allows_valid_messages() {
        let json = r#"{
            "simple": "Hello {name}",
            "plural": "{count, plural, one {# item} other {# items}}",
            "select": "{gender, select, male {He} female {She} other {They}}"
        }"#;
        let diags = check_json("/locales/en.json", json);
        assert!(diags.is_empty());
    }

    #[test]
    fn allows_plain_strings() {
        let json = r#"{"title": "Welcome", "description": "No variables here"}"#;
        let diags = check_json("/locales/en.json", json);
        assert!(diags.is_empty());
    }

    #[test]
    fn skips_non_i18n_files() {
        let json = r#"{"bad": "Hello {name"}"#;
        let diags = check_json("/config/settings.json", json);
        assert!(diags.is_empty()); // Not an i18n file
    }

    #[test]
    fn detects_by_locale_filename() {
        let json = r#"{"bad": "Hello {name"}"#;
        assert_eq!(check_json("/app/fr.json", json).len(), 1);
        assert_eq!(check_json("/app/en-US.json", json).len(), 1);
        assert_eq!(check_json("/app/zh_CN.json", json).len(), 1);
    }

    #[test]
    fn allows_i18next_interpolation() {
        let json = r#"{
            "day_one": "{{count}} day",
            "trial": "$t(day, {\"count\": {{days}} })",
            "mixed": "Hello {{name}}, you have {{count}} items"
        }"#;
        let diags = check_json("/locales/en.json", json);
        assert!(diags.is_empty());
    }

    #[test]
    fn validates_nested_objects() {
        let json = r#"{"section": {"greeting": "Hello {name"}}"#;
        let diags = check_json("/locales/en.json", json);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("section.greeting"));
    }
}
