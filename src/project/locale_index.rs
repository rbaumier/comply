//! LocaleIndex — cross-file index for i18n translation keys.
//!
//! Groups JSON translation files by directory and extracts all keys,
//! enabling rules like `i18n-json-identical-keys` to compare keys
//! across locales.

use serde_json::Value;
use rustc_hash::{FxHashMap, FxHashSet};
use std::path::{Path, PathBuf};

/// Index of translation keys across locale files.
#[derive(Debug, Default)]
pub struct LocaleIndex {
    /// Map: directory -> locale_name -> set of keys
    /// e.g., "/app/locales" -> "en" -> {"greeting", "farewell", "errors.notFound"}
    dirs: FxHashMap<PathBuf, FxHashMap<String, FxHashSet<String>>>,
}

impl LocaleIndex {
    pub fn new() -> Self {
        Self::default()
    }

    /// Build index from a list of JSON file paths and their contents.
    pub fn build(files: &[(&Path, &str)]) -> Self {
        let mut index = Self::new();

        for (path, content) in files {
            if !is_locale_file(path) {
                continue;
            }

            let Some(dir) = path.parent() else { continue };
            let Some(locale_name) = path.file_stem().and_then(|s| s.to_str()) else {
                continue;
            };

            let Ok(json) = serde_json::from_str::<Value>(content) else {
                continue;
            };

            let mut keys = FxHashSet::default();
            extract_keys(&json, &mut String::new(), &mut keys);

            index
                .dirs
                .entry(dir.to_path_buf())
                .or_default()
                .insert(locale_name.to_string(), keys);
        }

        index
    }

    /// Build index by scanning a directory for locale JSON files.
    pub fn build_from_dir(dir: &Path) -> Self {
        let mut index = Self::new();

        let Ok(entries) = std::fs::read_dir(dir) else {
            return index;
        };

        for entry in entries.flatten() {
            let path = entry.path();
            if !is_locale_file(&path) {
                continue;
            }

            let Some(locale_name) = path.file_stem().and_then(|s| s.to_str()) else {
                continue;
            };

            let Ok(content) = std::fs::read_to_string(&path) else {
                continue;
            };

            let Ok(json) = serde_json::from_str::<Value>(&content) else {
                continue;
            };

            let mut keys = FxHashSet::default();
            extract_keys(&json, &mut String::new(), &mut keys);

            index
                .dirs
                .entry(dir.to_path_buf())
                .or_default()
                .insert(locale_name.to_string(), keys);
        }

        index
    }

    /// Ensure the directory containing `path` is indexed.
    pub fn ensure_indexed(&mut self, path: &Path) {
        let Some(dir) = path.parent() else { return };
        if self.dirs.contains_key(dir) {
            return;
        }
        let dir_index = Self::build_from_dir(dir);
        self.dirs.extend(dir_index.dirs);
    }

    /// Get all locales in the same directory as the given file.
    pub fn get_locales_in_dir(&self, path: &Path) -> Option<&FxHashMap<String, FxHashSet<String>>> {
        let dir = path.parent()?;
        self.dirs.get(dir)
    }

    /// Get keys for a specific locale file.
    pub fn get_keys(&self, path: &Path) -> Option<&FxHashSet<String>> {
        let dir = path.parent()?;
        let locale = path.file_stem()?.to_str()?;
        self.dirs.get(dir)?.get(locale)
    }

    /// Find the base locale (en, or first alphabetically).
    pub fn get_base_locale(&self, path: &Path) -> Option<&str> {
        let locales = self.get_locales_in_dir(path)?;
        // Prefer "en" as base
        if locales.contains_key("en") {
            return Some("en");
        }
        // Fall back to first alphabetically
        locales.keys().map(|s| s.as_str()).min()
    }

    /// Get missing keys: keys in base locale but not in target locale.
    pub fn get_missing_keys(&self, path: &Path) -> Vec<String> {
        let Some(dir) = path.parent() else {
            return vec![];
        };
        let Some(locale) = path.file_stem().and_then(|s| s.to_str()) else {
            return vec![];
        };
        let Some(locales) = self.dirs.get(dir) else {
            return vec![];
        };

        // Find base locale
        let base = if locales.contains_key("en") {
            "en"
        } else {
            match locales.keys().map(|s| s.as_str()).min() {
                Some(b) => b,
                None => return vec![],
            }
        };

        // If this is the base locale, nothing is missing
        if locale == base {
            return vec![];
        }

        let Some(base_keys) = locales.get(base) else {
            return vec![];
        };
        let Some(target_keys) = locales.get(locale) else {
            return vec![];
        };

        let mut missing: Vec<String> = base_keys.difference(target_keys).cloned().collect();
        missing.sort();
        missing
    }

    /// Get extra keys: keys in target locale but not in base locale.
    pub fn get_extra_keys(&self, path: &Path) -> Vec<String> {
        let Some(dir) = path.parent() else {
            return vec![];
        };
        let Some(locale) = path.file_stem().and_then(|s| s.to_str()) else {
            return vec![];
        };
        let Some(locales) = self.dirs.get(dir) else {
            return vec![];
        };

        let base = if locales.contains_key("en") {
            "en"
        } else {
            match locales.keys().map(|s| s.as_str()).min() {
                Some(b) => b,
                None => return vec![],
            }
        };

        if locale == base {
            return vec![];
        }

        let Some(base_keys) = locales.get(base) else {
            return vec![];
        };
        let Some(target_keys) = locales.get(locale) else {
            return vec![];
        };

        let mut extra: Vec<String> = target_keys.difference(base_keys).cloned().collect();
        extra.sort();
        extra
    }

    pub fn is_empty(&self) -> bool {
        self.dirs.is_empty()
    }
}

fn is_locale_file(path: &Path) -> bool {
    if path.extension().and_then(|e| e.to_str()) != Some("json") {
        return false;
    }

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

    // Check filename patterns
    if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
        // 2-letter language code
        if stem.len() == 2 && stem.chars().all(|c| c.is_ascii_lowercase()) {
            return true;
        }
        // Language with region
        if (stem.len() == 5 || stem.len() == 6) && (stem.contains('-') || stem.contains('_')) {
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

fn extract_keys(value: &Value, prefix: &mut String, keys: &mut FxHashSet<String>) {
    match value {
        Value::Object(map) => {
            for (key, val) in map {
                let was_empty = prefix.is_empty();
                if !was_empty {
                    prefix.push('.');
                }
                prefix.push_str(key);

                if val.is_string() {
                    keys.insert(prefix.clone());
                } else {
                    extract_keys(val, prefix, keys);
                }

                // Restore prefix
                if was_empty {
                    prefix.clear();
                } else {
                    // Remove ".key"
                    let dot_pos = prefix.rfind('.').unwrap_or(0);
                    prefix.truncate(dot_pos);
                }
            }
        }
        Value::String(_) => {
            // Already handled by parent
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_flat_keys() {
        let json = r#"{"greeting": "Hello", "farewell": "Goodbye"}"#;
        let val: Value = serde_json::from_str(json).unwrap();
        let mut keys = FxHashSet::default();
        extract_keys(&val, &mut String::new(), &mut keys);
        assert!(keys.contains("greeting"));
        assert!(keys.contains("farewell"));
        assert_eq!(keys.len(), 2);
    }

    #[test]
    fn extracts_nested_keys() {
        let json = r#"{"errors": {"notFound": "Not found", "forbidden": "Forbidden"}}"#;
        let val: Value = serde_json::from_str(json).unwrap();
        let mut keys = FxHashSet::default();
        extract_keys(&val, &mut String::new(), &mut keys);
        assert!(keys.contains("errors.notFound"));
        assert!(keys.contains("errors.forbidden"));
        assert_eq!(keys.len(), 2);
    }

    #[test]
    fn builds_index_from_files() {
        let files: Vec<(&Path, &str)> = vec![
            (
                Path::new("/app/locales/en.json"),
                r#"{"greeting": "Hello", "farewell": "Goodbye"}"#,
            ),
            (
                Path::new("/app/locales/fr.json"),
                r#"{"greeting": "Bonjour"}"#,
            ),
        ];
        let index = LocaleIndex::build(&files);
        assert!(!index.is_empty());

        let en_keys = index.get_keys(Path::new("/app/locales/en.json")).unwrap();
        assert!(en_keys.contains("greeting"));
        assert!(en_keys.contains("farewell"));

        let fr_keys = index.get_keys(Path::new("/app/locales/fr.json")).unwrap();
        assert!(fr_keys.contains("greeting"));
        assert!(!fr_keys.contains("farewell"));
    }

    #[test]
    fn finds_missing_keys() {
        let files: Vec<(&Path, &str)> = vec![
            (
                Path::new("/app/locales/en.json"),
                r#"{"greeting": "Hello", "farewell": "Goodbye"}"#,
            ),
            (
                Path::new("/app/locales/fr.json"),
                r#"{"greeting": "Bonjour"}"#,
            ),
        ];
        let index = LocaleIndex::build(&files);

        let missing = index.get_missing_keys(Path::new("/app/locales/fr.json"));
        assert_eq!(missing, vec!["farewell"]);

        // Base locale has no missing keys
        let missing_en = index.get_missing_keys(Path::new("/app/locales/en.json"));
        assert!(missing_en.is_empty());
    }

    #[test]
    fn finds_extra_keys() {
        let files: Vec<(&Path, &str)> = vec![
            (
                Path::new("/app/locales/en.json"),
                r#"{"greeting": "Hello"}"#,
            ),
            (
                Path::new("/app/locales/fr.json"),
                r#"{"greeting": "Bonjour", "extra": "Extra"}"#,
            ),
        ];
        let index = LocaleIndex::build(&files);

        let extra = index.get_extra_keys(Path::new("/app/locales/fr.json"));
        assert_eq!(extra, vec!["extra"]);
    }

    #[test]
    fn prefers_en_as_base() {
        let files: Vec<(&Path, &str)> = vec![
            (Path::new("/app/locales/fr.json"), r#"{"a": "A"}"#),
            (Path::new("/app/locales/en.json"), r#"{"a": "A", "b": "B"}"#),
            (Path::new("/app/locales/de.json"), r#"{"a": "A"}"#),
        ];
        let index = LocaleIndex::build(&files);

        assert_eq!(
            index.get_base_locale(Path::new("/app/locales/fr.json")),
            Some("en")
        );
    }

    #[test]
    fn falls_back_to_alphabetical_base() {
        let files: Vec<(&Path, &str)> = vec![
            (Path::new("/app/locales/fr.json"), r#"{"a": "A", "b": "B"}"#),
            (Path::new("/app/locales/de.json"), r#"{"a": "A"}"#),
        ];
        let index = LocaleIndex::build(&files);

        // "de" comes before "fr" alphabetically
        assert_eq!(
            index.get_base_locale(Path::new("/app/locales/fr.json")),
            Some("de")
        );
    }
}
