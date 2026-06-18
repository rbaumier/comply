//! Detects placeholder mismatches between translations and base locale.
//!
//! For each key, extracts ICU placeholders (e.g., {name}, {count}) and
//! compares them with the base locale. Reports missing or extra placeholders.

use crate::diagnostic::{Diagnostic, Severity};
use crate::icu::extract_placeholders;
use crate::rules::backend::{CheckCtx, TextCheck};
use serde_json::Value;
use rustc_hash::{FxHashMap, FxHashSet};
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

fn extract_key_placeholders(
    value: &Value,
    prefix: &str,
    result: &mut FxHashMap<String, Vec<String>>,
) {
    match value {
        Value::Object(map) => {
            for (key, val) in map {
                let full_key = if prefix.is_empty() {
                    key.clone()
                } else {
                    format!("{prefix}.{key}")
                };
                extract_key_placeholders(val, &full_key, result);
            }
        }
        Value::String(s) => {
            // Always insert, even if empty - needed to detect missing/extra placeholders
            let placeholders = extract_placeholders(s);
            result.insert(prefix.to_string(), placeholders);
        }
        _ => {}
    }
}

fn find_base_locale(dir: &Path) -> Option<(String, FxHashMap<String, Vec<String>>)> {
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

    // Prefer "en" as base
    let base_name = if locales.contains(&"en".to_string()) {
        "en".to_string()
    } else {
        locales.first()?.clone()
    };

    let base_path = dir.join(format!("{base_name}.json"));
    let content = std::fs::read_to_string(&base_path).ok()?;
    let json: Value = serde_json::from_str(&content).ok()?;

    let mut placeholders = FxHashMap::default();
    extract_key_placeholders(&json, "", &mut placeholders);

    Some((base_name, placeholders))
}

struct PlaceholderMismatch {
    key: String,
    missing: Vec<String>,
    extra: Vec<String>,
    line: usize,
}

fn find_mismatches(
    current_placeholders: &FxHashMap<String, Vec<String>>,
    base_placeholders: &FxHashMap<String, Vec<String>>,
    source: &str,
) -> Vec<PlaceholderMismatch> {
    let mut mismatches = Vec::new();

    for (key, current) in current_placeholders {
        let Some(base) = base_placeholders.get(key) else {
            continue; // Key doesn't exist in base, handled by identical-keys rule
        };

        let current_set: FxHashSet<&String> = current.iter().collect();
        let base_set: FxHashSet<&String> = base.iter().collect();

        let missing: Vec<String> = base_set
            .difference(&current_set)
            .map(|s| (*s).clone())
            .collect();
        let extra: Vec<String> = current_set
            .difference(&base_set)
            .map(|s| (*s).clone())
            .collect();

        if !missing.is_empty() || !extra.is_empty() {
            let line = find_line_for_key(source, key);
            mismatches.push(PlaceholderMismatch {
                key: key.clone(),
                missing,
                extra,
                line,
            });
        }
    }

    mismatches
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

        let Some((base_name, base_placeholders)) = find_base_locale(dir) else {
            return vec![];
        };

        // Don't check base locale against itself
        if current_locale == base_name {
            return vec![];
        }

        let Ok(json) = serde_json::from_str::<Value>(ctx.source) else {
            return vec![];
        };

        let mut current_placeholders = FxHashMap::default();
        extract_key_placeholders(&json, "", &mut current_placeholders);

        let mismatches = find_mismatches(&current_placeholders, &base_placeholders, ctx.source);

        mismatches
            .into_iter()
            .map(|m| {
                let mut msg_parts = Vec::new();
                if !m.missing.is_empty() {
                    msg_parts.push(format!("missing: {{{}}}", m.missing.join("}, {")));
                }
                if !m.extra.is_empty() {
                    msg_parts.push(format!("extra: {{{}}}", m.extra.join("}, {")));
                }

                Diagnostic {
                    path: std::sync::Arc::clone(&ctx.path_arc),
                    line: m.line,
                    column: 1,
                    rule_id: super::META.id.into(),
                    message: format!(
                        "Placeholder mismatch in `{}`: {}",
                        m.key,
                        msg_parts.join("; ")
                    ),
                    severity: Severity::Error,
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
    fn detects_missing_placeholder() {
        let dir = setup_locales(&[
            ("en.json", r#"{"greeting": "Hello {name}!"}"#),
            ("fr.json", r#"{"greeting": "Bonjour!"}"#),
        ]);

        let diags = check_file(&dir, "fr.json");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("missing"));
        assert!(diags[0].message.contains("name"));
    }

    #[test]
    fn detects_extra_placeholder() {
        let dir = setup_locales(&[
            ("en.json", r#"{"greeting": "Hello!"}"#),
            ("fr.json", r#"{"greeting": "Bonjour {name}!"}"#),
        ]);

        let diags = check_file(&dir, "fr.json");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("extra"));
        assert!(diags[0].message.contains("name"));
    }

    #[test]
    fn detects_different_placeholder_names() {
        let dir = setup_locales(&[
            ("en.json", r#"{"greeting": "Hello {name}!"}"#),
            ("fr.json", r#"{"greeting": "Bonjour {userName}!"}"#),
        ]);

        let diags = check_file(&dir, "fr.json");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("missing"));
        assert!(diags[0].message.contains("name"));
        assert!(diags[0].message.contains("extra"));
        assert!(diags[0].message.contains("userName"));
    }

    #[test]
    fn allows_identical_placeholders() {
        let dir = setup_locales(&[
            (
                "en.json",
                r#"{"greeting": "Hello {name}, you have {count} messages"}"#,
            ),
            (
                "fr.json",
                r#"{"greeting": "Bonjour {name}, vous avez {count} messages"}"#,
            ),
        ]);

        let diags = check_file(&dir, "fr.json");
        assert!(diags.is_empty());
    }

    #[test]
    fn allows_different_order() {
        let dir = setup_locales(&[
            ("en.json", r#"{"msg": "{a} then {b}"}"#),
            ("fr.json", r#"{"msg": "{b} puis {a}"}"#),
        ]);

        let diags = check_file(&dir, "fr.json");
        assert!(diags.is_empty());
    }

    #[test]
    fn base_locale_not_checked() {
        let dir = setup_locales(&[
            ("en.json", r#"{"greeting": "Hello {name}!"}"#),
            ("fr.json", r#"{"greeting": "Bonjour!"}"#),
        ]);

        let diags = check_file(&dir, "en.json");
        assert!(diags.is_empty()); // en is base, not compared
    }

    #[test]
    fn handles_plural_placeholders() {
        let dir = setup_locales(&[
            (
                "en.json",
                r#"{"items": "{count, plural, one {# item} other {# items}}"}"#,
            ),
            (
                "fr.json",
                r#"{"items": "{nombre, plural, one {# item} other {# items}}"}"#,
            ),
        ]);

        let diags = check_file(&dir, "fr.json");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("count"));
        assert!(diags[0].message.contains("nombre"));
    }
}
