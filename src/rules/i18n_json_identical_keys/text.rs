//! Detects missing translation keys compared to the base locale.
//!
//! Scans all locale files in the same directory, determines the base locale
//! (preferring `en.json`), and reports keys missing from other locales.

use crate::diagnostic::{Diagnostic, Severity};
use crate::project::LocaleIndex;
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

fn is_locale_dir(path: &std::path::Path) -> bool {
    let path_str = path.to_string_lossy();
    path_str.contains("/locales/")
        || path_str.contains("/translations/")
        || path_str.contains("/i18n/")
        || path_str.contains("/lang/")
        || path_str.contains("/messages/")
}

fn is_locale_filename(path: &std::path::Path) -> bool {
    if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
        // 2-letter language code
        if stem.len() == 2 && stem.chars().all(|c| c.is_ascii_lowercase()) {
            return true;
        }
        // Language with region
        if (stem.len() == 5 || stem.len() == 6) && (stem.contains('-') || stem.contains('_')) {
            return true;
        }
    }
    false
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        // Only check JSON files
        if ctx.path.extension().and_then(|e| e.to_str()) != Some("json") {
            return vec![];
        }

        // Only check locale files
        if !is_locale_dir(ctx.path) && !is_locale_filename(ctx.path) {
            return vec![];
        }

        let Some(dir) = ctx.path.parent() else {
            return vec![];
        };

        // Build index for this directory
        let index = LocaleIndex::build_from_dir(dir);

        // Get missing keys for this file
        let missing = index.get_missing_keys(ctx.path);

        if missing.is_empty() {
            return vec![];
        }

        // Group into single diagnostic with all missing keys
        let keys_str = if missing.len() <= 5 {
            missing.join(", ")
        } else {
            format!(
                "{}, ... and {} more",
                missing[..5].join(", "),
                missing.len() - 5
            )
        };

        vec![Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: 1,
            column: 1,
            rule_id: super::META.id.into(),
            message: format!(
                "Missing {} translation key(s) from base locale: {}",
                missing.len(),
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
    fn detects_missing_keys() {
        let dir = setup_locales(&[
            ("en.json", r#"{"greeting": "Hello", "farewell": "Goodbye"}"#),
            ("fr.json", r#"{"greeting": "Bonjour"}"#),
        ]);

        let diags = check_file(&dir, "fr.json");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("farewell"));
        assert!(diags[0].message.contains("Missing 1"));
    }

    #[test]
    fn detects_multiple_missing_keys() {
        let dir = setup_locales(&[
            (
                "en.json",
                r#"{"a": "A", "b": "B", "c": "C", "d": "D"}"#,
            ),
            ("fr.json", r#"{"a": "A"}"#),
        ]);

        let diags = check_file(&dir, "fr.json");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("Missing 3"));
    }

    #[test]
    fn allows_complete_translation() {
        let dir = setup_locales(&[
            ("en.json", r#"{"greeting": "Hello", "farewell": "Goodbye"}"#),
            ("fr.json", r#"{"greeting": "Bonjour", "farewell": "Au revoir"}"#),
        ]);

        let diags = check_file(&dir, "fr.json");
        assert!(diags.is_empty());
    }

    #[test]
    fn base_locale_has_no_missing_keys() {
        let dir = setup_locales(&[
            ("en.json", r#"{"greeting": "Hello"}"#),
            ("fr.json", r#"{"greeting": "Bonjour", "extra": "Extra"}"#),
        ]);

        let diags = check_file(&dir, "en.json");
        assert!(diags.is_empty()); // en is base, nothing to compare against
    }

    #[test]
    fn handles_nested_keys() {
        let dir = setup_locales(&[
            (
                "en.json",
                r#"{"errors": {"notFound": "Not found", "forbidden": "Forbidden"}}"#,
            ),
            ("fr.json", r#"{"errors": {"notFound": "Non trouvé"}}"#),
        ]);

        let diags = check_file(&dir, "fr.json");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("errors.forbidden"));
    }
}
