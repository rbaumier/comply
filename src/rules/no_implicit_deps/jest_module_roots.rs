//! Jest `modulePaths` / `moduleDirectories` resolution.
//!
//! Jest lets a project add extra module-resolution roots through
//! `modulePaths` (paths relative to `rootDir`) and `moduleDirectories`
//! (directory names, often written with a `<rootDir>` token). A bare specifier
//! whose first segments live under one of those roots resolves to in-repo
//! source, not an npm package — e.g. `import x from "app/core/foo"` resolves to
//! `<rootDir>/public/app/core/foo` under `modulePaths: ["public"]`. The config
//! lives in `jest.config.{js,ts,cjs,mjs,json}` or the `"jest"` key of
//! `package.json`, with `rootDir` defaulting to the config's own directory.
//!
//! `no-implicit-deps` reads these roots to avoid flagging such imports. The
//! check is grounded on disk: a configured root only suppresses a specifier
//! when the joined path actually resolves to a local source file, so a genuine
//! undeclared package still fires.

use std::path::{Path, PathBuf};

/// Jest config files that may declare `modulePaths` / `moduleDirectories`.
const CONFIG_FILES: &[&str] = &[
    "jest.config.js",
    "jest.config.ts",
    "jest.config.cjs",
    "jest.config.mjs",
    "jest.config.json",
];

/// True if a bare `spec` resolves to local source through a Jest `modulePaths`
/// or `moduleDirectories` root declared in any config between `importer`'s
/// directory and `stop_at` (inclusive). Walks no further than `stop_at` so the
/// scan never escapes the project root.
pub(super) fn resolves_via_jest_module_roots(
    importer: &Path,
    spec: &str,
    stop_at: Option<&Path>,
) -> bool {
    let mut dir = importer.parent();
    while let Some(d) = dir {
        for root in module_roots_in_dir(d) {
            if local_source_exists(&root.join(spec)) {
                return true;
            }
        }
        if stop_at == Some(d) {
            break;
        }
        dir = d.parent();
    }
    false
}

/// Collect the absolute module-resolution roots declared by a Jest config in
/// directory `d` (a `jest.config.*` file or the `"jest"` key of
/// `package.json`). `rootDir` defaults to `d`.
fn module_roots_in_dir(d: &Path) -> Vec<PathBuf> {
    let mut raws: Vec<String> = Vec::new();
    for file in CONFIG_FILES {
        if let Ok(raw) = std::fs::read_to_string(d.join(file)) {
            raws.push(raw);
        }
    }
    if let Ok(raw) = std::fs::read_to_string(d.join("package.json")) {
        if let Some(jest) = jest_block(&raw) {
            raws.push(jest.to_string());
        }
    }

    let mut roots = Vec::new();
    for raw in &raws {
        for key in ["modulePaths", "moduleDirectories"] {
            for value in string_array_values(raw, key) {
                // `node_modules` is the npm-package search root; honoring it
                // would suppress every genuine undeclared package.
                if value == "node_modules" {
                    continue;
                }
                roots.push(resolve_root(d, &value));
            }
        }
    }
    roots
}

/// Resolve a configured root against `root_dir`, stripping a leading
/// `<rootDir>` token and normalizing an absolute-looking root to be relative to
/// `root_dir` (Jest treats both `"public"` and `"<rootDir>/public"` the same).
fn resolve_root(root_dir: &Path, value: &str) -> PathBuf {
    let trimmed = value
        .trim_start_matches("<rootDir>")
        .trim_start_matches('/');
    root_dir.join(trimmed)
}

/// Slice of `raw` holding the value of the `"jest"` key in a `package.json`,
/// from its opening `{` to the matching close brace. Returns `None` when the
/// key is absent or its value is not an object.
fn jest_block(raw: &str) -> Option<&str> {
    let bytes = raw.as_bytes();
    let mut search_from = 0;
    while let Some(rel) = raw[search_from..].find("\"jest\"") {
        let key_end = search_from + rel + "\"jest\"".len();
        search_from = key_end;
        let Some(colon) = next_non_ws(bytes, key_end) else {
            continue;
        };
        if bytes[colon] != b':' {
            continue;
        }
        let Some(open) = next_non_ws(bytes, colon + 1) else {
            continue;
        };
        if bytes[open] != b'{' {
            continue;
        }
        let close = matching_brace(bytes, open)?;
        return raw.get(open..=close);
    }
    None
}

/// Extract every string-literal element of the array assigned to `key` in
/// `source` (`key: [ "a", "b" ]`). Scans every occurrence so configs that
/// repeat a key (or hold it inside an overridden `projects` block) all
/// contribute their roots.
fn string_array_values(source: &str, key: &str) -> Vec<String> {
    let bytes = source.as_bytes();
    let mut out = Vec::new();
    let mut search_from = 0;
    while let Some(rel) = source[search_from..].find(key) {
        let kw_start = search_from + rel;
        let kw_end = kw_start + key.len();
        search_from = kw_end;

        // The key must be a standalone property name, not a substring of a
        // larger identifier (`moduleDirectoriesExtra`) and not a quoted value.
        let prev = kw_start.checked_sub(1).map(|i| bytes[i]);
        if prev.is_some_and(is_ident_char) {
            continue;
        }
        if kw_end < bytes.len() && is_ident_char(bytes[kw_end]) {
            continue;
        }

        // Allow an optional closing quote (`"modulePaths":`) before the colon.
        let mut after_key = kw_end;
        if after_key < bytes.len() && (bytes[after_key] == b'"' || bytes[after_key] == b'\'') {
            after_key += 1;
        }
        let Some(colon) = next_non_ws(bytes, after_key) else {
            continue;
        };
        if bytes[colon] != b':' {
            continue;
        }
        let Some(open) = next_non_ws(bytes, colon + 1) else {
            continue;
        };
        if bytes[open] != b'[' {
            continue;
        }
        collect_array_strings(source, open, &mut out);
    }
    out
}

/// Record each string literal directly inside the array whose `[` is at `open`,
/// stopping at the matching `]`. Nested arrays/objects are skipped.
fn collect_array_strings(source: &str, open: usize, out: &mut Vec<String>) {
    let bytes = source.as_bytes();
    let mut i = open + 1;
    let mut depth = 1usize;
    while i < bytes.len() {
        match bytes[i] {
            b'[' | b'{' | b'(' => {
                depth += 1;
                i += 1;
            }
            b']' | b'}' | b')' => {
                depth -= 1;
                if depth == 0 {
                    return;
                }
                i += 1;
            }
            b'"' | b'\'' if depth == 1 => {
                let quote = bytes[i];
                let start = i + 1;
                let mut j = start;
                while j < bytes.len() && bytes[j] != quote {
                    if bytes[j] == b'\\' {
                        j += 1;
                    }
                    j += 1;
                }
                if let Some(s) = source.get(start..j) {
                    out.push(s.to_string());
                }
                i = j + 1;
            }
            _ => i += 1,
        }
    }
}

/// Index of the `}` matching the `{` at `open`, honoring string literals so
/// braces inside strings do not unbalance the count.
fn matching_brace(bytes: &[u8], open: usize) -> Option<usize> {
    let mut i = open + 1;
    let mut depth = 1usize;
    while i < bytes.len() {
        match bytes[i] {
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            b'"' | b'\'' => {
                let quote = bytes[i];
                i += 1;
                while i < bytes.len() && bytes[i] != quote {
                    if bytes[i] == b'\\' {
                        i += 1;
                    }
                    i += 1;
                }
            }
            _ => {}
        }
        i += 1;
    }
    None
}

fn next_non_ws(bytes: &[u8], from: usize) -> Option<usize> {
    let mut i = from;
    while i < bytes.len() && matches!(bytes[i], b' ' | b'\t' | b'\r' | b'\n') {
        i += 1;
    }
    (i < bytes.len()).then_some(i)
}

fn is_ident_char(c: u8) -> bool {
    c.is_ascii_alphanumeric() || c == b'_' || c == b'$'
}

/// Extensions a JS/TS resolver appends to an extension-less module specifier.
const SOURCE_EXTENSIONS: &[&str] =
    &["ts", "tsx", "d.ts", "mts", "cts", "js", "jsx", "mjs", "cjs", "vue", "json"];

/// True if `candidate` (an extension-less module path) points at an existing
/// local source file — directly, with a source extension appended, or as a
/// directory containing an `index.*` entry. Also matches a non-source file
/// (e.g. an `img/` asset) resolved by Jest's `moduleNameMapper`/transforms.
fn local_source_exists(candidate: &Path) -> bool {
    if candidate.is_file() {
        return true;
    }
    if let (Some(name), Some(parent)) = (
        candidate.file_name().and_then(|n| n.to_str()),
        candidate.parent(),
    ) {
        for ext in SOURCE_EXTENSIONS {
            if parent.join(format!("{name}.{ext}")).is_file() {
                return true;
            }
        }
    }
    if candidate.is_dir()
        && SOURCE_EXTENSIONS
            .iter()
            .any(|ext| candidate.join(format!("index.{ext}")).is_file())
    {
        return true;
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    fn values(source: &str, key: &str) -> Vec<String> {
        string_array_values(source, key)
    }

    #[test]
    fn extracts_module_paths_array() {
        let src = r#"module.exports = { modulePaths: ['public', 'node_modules'] };"#;
        assert_eq!(values(src, "modulePaths"), vec!["public", "node_modules"]);
    }

    #[test]
    fn extracts_quoted_key_in_json() {
        let src = r#"{ "modulePaths": ["public"] }"#;
        assert_eq!(values(src, "modulePaths"), vec!["public"]);
    }

    #[test]
    fn ignores_substring_key() {
        let src = r#"{ modulePathsExtra: ['nope'] }"#;
        assert!(values(src, "modulePaths").is_empty());
    }

    #[test]
    fn jest_block_isolates_object() {
        let raw = r#"{ "name": "x", "jest": { "modulePaths": ["public"] }, "scripts": {} }"#;
        let block = jest_block(raw).unwrap();
        assert!(block.contains("modulePaths"));
        assert!(!block.contains("scripts"));
    }

    #[test]
    fn resolve_root_strips_root_dir_token() {
        let base = Path::new("/repo");
        assert_eq!(resolve_root(base, "public"), base.join("public"));
        assert_eq!(
            resolve_root(base, "<rootDir>/public/app"),
            base.join("public/app")
        );
    }
}
