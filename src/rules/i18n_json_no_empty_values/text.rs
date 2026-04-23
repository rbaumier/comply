//! Detects empty string values in i18n JSON files.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};
use serde_json::Value;
use std::path::Path;

#[derive(Debug)]
pub struct Check;

fn is_locale_file(path: &Path) -> bool {
    if path.extension().and_then(|e| e.to_str()) != Some("json") {
        return false;
    }
    let path_str = path.to_string_lossy();
    if path_str.contains("/locales/")
        || path_str.contains("/translations/")
        || path_str.contains("/i18n/")
        || path_str.contains("/lang/")
        || path_str.contains("/messages/")
    {
        return true;
    }
    if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
        if stem.len() == 2 && stem.chars().all(|c| c.is_ascii_lowercase()) {
            return true;
        }
        if (stem.len() == 5 || stem.len() == 6) && (stem.contains('-') || stem.contains('_')) {
            return true;
        }
    }
    false
}

fn find_empty_values(value: &Value, prefix: &str, results: &mut Vec<String>) {
    match value {
        Value::Object(map) => {
            for (key, val) in map {
                let full_key = if prefix.is_empty() {
                    key.clone()
                } else {
                    format!("{prefix}.{key}")
                };
                find_empty_values(val, &full_key, results);
            }
        }
        Value::String(s) if s.is_empty() => {
            results.push(prefix.to_string());
        }
        _ => {}
    }
}

fn find_line_for_key(source: &str, key: &str) -> usize {
    let leaf_key = key.rsplit('.').next().unwrap_or(key);
    let search = format!("\"{leaf_key}\"");
    for (i, line) in source.lines().enumerate() {
        if line.contains(&search) {
            return i + 1;
        }
    }
    1
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        if !is_locale_file(ctx.path) {
            return vec![];
        }

        let Ok(json) = serde_json::from_str::<Value>(ctx.source) else {
            return vec![];
        };

        let mut empty_keys = Vec::new();
        find_empty_values(&json, "", &mut empty_keys);

        empty_keys
            .into_iter()
            .map(|key| {
                let line = find_line_for_key(ctx.source, &key);
                Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line,
                    column: 1,
                    rule_id: super::META.id.into(),
                    message: format!("Empty translation value for `{key}`"),
                    severity: Severity::Warning,
                    span: None,
                }
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn check(path: &str, content: &str) -> Vec<Diagnostic> {
        let ctx = crate::rules::backend::CheckCtx::for_test(Path::new(path), content);
        Check.check(&ctx)
    }

    #[test]
    fn detects_empty_value() {
        let json = r#"{"greeting": "", "farewell": "Goodbye"}"#;
        let diags = check("/locales/en.json", json);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("greeting"));
    }

    #[test]
    fn detects_multiple_empty_values() {
        let json = r#"{"a": "", "b": "ok", "c": ""}"#;
        let diags = check("/locales/en.json", json);
        assert_eq!(diags.len(), 2);
    }

    #[test]
    fn detects_nested_empty_value() {
        let json = r#"{"errors": {"notFound": "", "forbidden": "No"}}"#;
        let diags = check("/locales/en.json", json);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("errors.notFound"));
    }

    #[test]
    fn allows_non_empty_values() {
        let json = r#"{"greeting": "Hello", "farewell": "Goodbye"}"#;
        let diags = check("/locales/en.json", json);
        assert!(diags.is_empty());
    }

    #[test]
    fn skips_non_locale_files() {
        let json = r#"{"empty": ""}"#;
        let diags = check("/config/settings.json", json);
        assert!(diags.is_empty());
    }
}
