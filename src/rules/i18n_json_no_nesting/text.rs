//! Detects nested objects in i18n JSON files — enforces flat key structure.
//!
//! Prefer: `{"errors.notFound": "Not found"}`
//! Over:   `{"errors": {"notFound": "Not found"}}`

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

fn find_nested_objects(value: &Value, prefix: &str, results: &mut Vec<String>) {
    if let Value::Object(map) = value {
        for (key, val) in map {
            let full_key = if prefix.is_empty() {
                key.clone()
            } else {
                format!("{prefix}.{key}")
            };

            if let Value::Object(_) = val {
                // This key has a nested object
                results.push(full_key.clone());
                // Continue to find deeper nesting
                find_nested_objects(val, &full_key, results);
            }
        }
    }
}

fn find_line_for_key(source: &str, key: &str) -> usize {
    // For nested keys like "errors.notFound", search for the parent "errors"
    let search_key = key.split('.').next().unwrap_or(key);
    let search = format!("\"{search_key}\"");
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

        let mut nested_keys = Vec::new();
        find_nested_objects(&json, "", &mut nested_keys);

        if nested_keys.is_empty() {
            return vec![];
        }

        // Report only top-level nested keys to avoid noise
        let top_level: Vec<&String> = nested_keys
            .iter()
            .filter(|k| !k.contains('.'))
            .collect();

        if top_level.is_empty() {
            // All nesting is deeper, report the first level found
            if let Some(first) = nested_keys.first() {
                let root = first.split('.').next().unwrap_or(first);
                return vec![Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: find_line_for_key(ctx.source, root),
                    column: 1,
                    rule_id: super::META.id.into(),
                    message: format!(
                        "Nested objects detected — flatten keys like `\"{first}\": \"...\"` instead"
                    ),
                    severity: Severity::Warning,
                    span: None,
                }];
            }
            return vec![];
        }

        // Group into single diagnostic
        let keys_str = if top_level.len() <= 5 {
            top_level.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", ")
        } else {
            format!(
                "{}, ... and {} more",
                top_level[..5].iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", "),
                top_level.len() - 5
            )
        };

        vec![Diagnostic {
            path: ctx.path.to_path_buf(),
            line: 1,
            column: 1,
            rule_id: super::META.id.into(),
            message: format!(
                "Nested objects detected: {} — use flat keys like `\"errors.notFound\"` instead",
                keys_str
            ),
            severity: Severity::Warning,
            span: None,
        }]
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
    fn detects_nested_object() {
        let json = r#"{"errors": {"notFound": "Not found"}}"#;
        let diags = check("/locales/en.json", json);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("errors"));
    }

    #[test]
    fn detects_multiple_nested() {
        let json = r#"{"errors": {"a": "A"}, "messages": {"b": "B"}}"#;
        let diags = check("/locales/en.json", json);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("errors"));
        assert!(diags[0].message.contains("messages"));
    }

    #[test]
    fn detects_deep_nesting() {
        let json = r#"{"a": {"b": {"c": "deep"}}}"#;
        let diags = check("/locales/en.json", json);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn allows_flat_keys() {
        let json = r#"{"greeting": "Hello", "errors.notFound": "Not found"}"#;
        let diags = check("/locales/en.json", json);
        assert!(diags.is_empty());
    }

    #[test]
    fn allows_flat_with_dots() {
        let json = r#"{"a.b.c": "nested key", "x.y": "another"}"#;
        let diags = check("/locales/en.json", json);
        assert!(diags.is_empty());
    }

    #[test]
    fn skips_non_locale_files() {
        let json = r#"{"nested": {"key": "value"}}"#;
        let diags = check("/config/settings.json", json);
        assert!(diags.is_empty());
    }
}
