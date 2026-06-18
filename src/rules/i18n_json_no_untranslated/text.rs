//! Detects translation values identical to the base locale (likely untranslated).

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};
use serde_json::Value;
use rustc_hash::FxHashMap;
use std::path::Path;

#[derive(Debug)]
pub struct Check;

fn is_locale_dir(path: &Path) -> bool {
    let path_str = path.to_string_lossy();
    path_str.contains("/locales/")
        || path_str.contains("/translations/")
        || path_str.contains("/i18n/")
        || path_str.contains("/lang/")
        || path_str.contains("/messages/")
}

fn is_locale_filename(path: &Path) -> bool {
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

fn extract_string_values(value: &Value, prefix: &str, result: &mut FxHashMap<String, String>) {
    match value {
        Value::Object(map) => {
            for (key, val) in map {
                let full_key = if prefix.is_empty() {
                    key.clone()
                } else {
                    format!("{prefix}.{key}")
                };
                extract_string_values(val, &full_key, result);
            }
        }
        Value::String(s) => {
            result.insert(prefix.to_string(), s.clone());
        }
        _ => {}
    }
}

fn find_base_locale_values(dir: &Path) -> Option<(String, FxHashMap<String, String>)> {
    let entries = std::fs::read_dir(dir).ok()?;

    let mut locales: Vec<String> = entries
        .flatten()
        .filter_map(|e| {
            let path = e.path();
            if path.extension()?.to_str()? == "json" {
                path.file_stem()?.to_str().map(|s| s.to_string())
            } else {
                None
            }
        })
        .collect();

    locales.sort();

    let base_name = if locales.contains(&"en".to_string()) {
        "en".to_string()
    } else {
        locales.first()?.clone()
    };

    let base_path = dir.join(format!("{base_name}.json"));
    let content = std::fs::read_to_string(&base_path).ok()?;
    let json: Value = serde_json::from_str(&content).ok()?;

    let mut values = FxHashMap::default();
    extract_string_values(&json, "", &mut values);

    Some((base_name, values))
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

fn is_likely_untranslatable(value: &str) -> bool {
    // Skip very short strings, numbers, URLs, emails
    if value.len() <= 2 {
        return true;
    }
    if value.parse::<f64>().is_ok() {
        return true;
    }
    // Version strings like "1.0.0"
    if value.chars().all(|c| c.is_ascii_digit() || c == '.') {
        return true;
    }
    if value.starts_with("http://") || value.starts_with("https://") {
        return true;
    }
    if value.contains('@') && value.contains('.') {
        return true;
    }
    // Skip strings that are just placeholders
    if value.starts_with('{') && value.ends_with('}') && !value.contains(' ') {
        return true;
    }
    // Single words are often proper nouns or technical terms (Discord, CLI, macOS)
    if !value.contains(' ') {
        return true;
    }
    false
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        if ctx.path.extension().and_then(|e| e.to_str()) != Some("json") {
            return vec![];
        }

        if !is_locale_dir(ctx.path) && !is_locale_filename(ctx.path) {
            return vec![];
        }

        let Some(dir) = ctx.path.parent() else {
            return vec![];
        };

        let current_locale = ctx.path.file_stem().and_then(|s| s.to_str()).unwrap_or("");

        let Some((base_name, base_values)) = find_base_locale_values(dir) else {
            return vec![];
        };

        // Don't check base locale against itself
        if current_locale == base_name {
            return vec![];
        }

        let Ok(json) = serde_json::from_str::<Value>(ctx.source) else {
            return vec![];
        };

        let mut current_values = FxHashMap::default();
        extract_string_values(&json, "", &mut current_values);

        let mut untranslated = Vec::new();

        for (key, current_value) in &current_values {
            if let Some(base_value) = base_values.get(key) {
                // Skip if identical and not likely untranslatable
                if current_value == base_value && !is_likely_untranslatable(current_value) {
                    untranslated.push(key.clone());
                }
            }
        }

        untranslated.sort();

        untranslated
            .into_iter()
            .map(|key| {
                let line = find_line_for_key(ctx.source, &key);
                Diagnostic {
                    path: std::sync::Arc::clone(&ctx.path_arc),
                    line,
                    column: 1,
                    rule_id: super::META.id.into(),
                    message: format!("Untranslated value for `{key}` — identical to base locale"),
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
    use std::fs;
    use tempfile::TempDir;

    fn setup_locales(files: &[(&str, &str)]) -> TempDir {
        let dir = TempDir::new().unwrap();
        let locales_dir = dir.path().join("locales");
        fs::create_dir_all(&locales_dir).unwrap();

        for (name, content) in files {
            fs::write(locales_dir.join(name), content).unwrap();
        }

        dir
    }

    fn check_file(dir: &TempDir, filename: &str) -> Vec<Diagnostic> {
        let path = dir.path().join("locales").join(filename);
        let content = fs::read_to_string(&path).unwrap();
        let ctx = crate::rules::backend::CheckCtx::for_test(&path, &content);
        Check.check(&ctx)
    }

    #[test]
    fn detects_untranslated_value() {
        let dir = setup_locales(&[
            ("en.json", r#"{"greeting": "Hello world"}"#),
            ("fr.json", r#"{"greeting": "Hello world"}"#),
        ]);

        let diags = check_file(&dir, "fr.json");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("greeting"));
    }

    #[test]
    fn allows_translated_value() {
        let dir = setup_locales(&[
            ("en.json", r#"{"greeting": "Hello"}"#),
            ("fr.json", r#"{"greeting": "Bonjour"}"#),
        ]);

        let diags = check_file(&dir, "fr.json");
        assert!(diags.is_empty());
    }

    #[test]
    fn allows_short_strings() {
        let dir = setup_locales(&[
            ("en.json", r#"{"ok": "OK", "no": "No"}"#),
            ("fr.json", r#"{"ok": "OK", "no": "No"}"#),
        ]);

        let diags = check_file(&dir, "fr.json");
        assert!(diags.is_empty()); // Short strings are likely intentional
    }

    #[test]
    fn allows_urls() {
        let dir = setup_locales(&[
            ("en.json", r#"{"link": "https://example.com"}"#),
            ("fr.json", r#"{"link": "https://example.com"}"#),
        ]);

        let diags = check_file(&dir, "fr.json");
        assert!(diags.is_empty());
    }

    #[test]
    fn allows_numbers() {
        let dir = setup_locales(&[
            ("en.json", r#"{"version": "1.0.0"}"#),
            ("fr.json", r#"{"version": "1.0.0"}"#),
        ]);

        let diags = check_file(&dir, "fr.json");
        assert!(diags.is_empty());
    }

    #[test]
    fn base_locale_not_checked() {
        let dir = setup_locales(&[
            ("en.json", r#"{"greeting": "Hello"}"#),
            ("fr.json", r#"{"greeting": "Hello"}"#),
        ]);

        let diags = check_file(&dir, "en.json");
        assert!(diags.is_empty());
    }

    #[test]
    fn detects_multiple_untranslated() {
        let dir = setup_locales(&[
            (
                "en.json",
                r#"{"a": "Hello there friend", "b": "Welcome to the app", "c": "Bonjour"}"#,
            ),
            (
                "fr.json",
                r#"{"a": "Hello there friend", "b": "Welcome to the app", "c": "Salut"}"#,
            ),
        ]);

        let diags = check_file(&dir, "fr.json");
        assert_eq!(diags.len(), 2);
    }

    #[test]
    fn allows_single_word_proper_nouns() {
        let dir = setup_locales(&[
            (
                "en.json",
                r#"{"brand": "Discord", "os": "macOS", "tool": "CLI"}"#,
            ),
            (
                "fr.json",
                r#"{"brand": "Discord", "os": "macOS", "tool": "CLI"}"#,
            ),
        ]);

        let diags = check_file(&dir, "fr.json");
        assert!(diags.is_empty());
    }
}
