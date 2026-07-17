//! Honors the file-exclusion mechanisms of the project's own ESLint setup so
//! comply skips the same files ESLint would never lint (generated code,
//! `.d.ts`, fixtures, build output).
//!
//! Covered:
//! - flat config global ignores: an object whose **only** key is `ignores`
//!   in `eslint.config.{js,mjs,cjs,ts,mts,cts}` (`load`).
//! - `ignorePatterns` in `.eslintrc.{json,js,cjs,yaml,yml}` and in
//!   `package.json` → `eslintConfig` (`load`).
//! - `.eslintignore` / `.eslint-ignore` files (gitignore syntax; often a
//!   symlink to `.prettierignore`) via `load_ignore_files`. These routinely
//!   list non-JS globs (`*.rs`, `*.py`) ESLint never lints, so the caller
//!   applies the returned matcher only to the TypeScript family — see
//!   `crate::files`.
//!
//! Static extraction only — comply does not execute the config JS. String
//! literals and `...NAME` spreads of a same-file `const NAME = [literals]` are
//! resolved; patterns from imported variables, runtime file reads, or computed
//! expressions are not. Fall back to `.complyignore` for those.

use rustc_hash::FxHashMap;
use std::path::{Path, PathBuf};

use ignore::gitignore::{Gitignore, GitignoreBuilder};

use crate::rules::walker::walk_tree;

const FLAT_CONFIGS: &[&str] = &[
    "eslint.config.js",
    "eslint.config.mjs",
    "eslint.config.cjs",
    "eslint.config.ts",
    "eslint.config.mts",
    "eslint.config.cts",
];

/// ESLint's file-based path-exclusion files, gitignore syntax. A project often
/// symlinks `.eslintignore` to `.prettierignore`; `std::fs` follows the symlink
/// transparently when the matcher reads them.
const IGNORE_FILES: &[&str] = &[".eslintignore", ".eslint-ignore"];

/// Build a matcher from ESLint's config-based ignore mechanisms, anchored at
/// the nearest config root at or above `scan_root`. Returns `None` when no
/// ESLint config is found or it declares no ignore patterns.
pub fn load(scan_root: &Path) -> Option<Gitignore> {
    let root = config_root(scan_root)?;
    let mut patterns = Vec::new();
    collect_flat_config(&root, &mut patterns);
    collect_eslintrc(&root, &mut patterns);
    collect_package_json(&root, &mut patterns);
    if patterns.is_empty() {
        return None;
    }
    let mut builder = GitignoreBuilder::new(&root);
    for pattern in &patterns {
        // A malformed glob is skipped rather than aborting the whole matcher.
        let _ = builder.add_line(None, pattern);
    }
    builder.build().ok()
}

/// Build a matcher from the project's `.eslintignore` / `.eslint-ignore` files
/// (gitignore syntax), anchored at the nearest ancestor of `scan_root` that
/// holds one. Returns `None` when neither file is found.
///
/// Unlike [`load`], these files exist independently of any ESLint config, so
/// the search keys on the ignore file itself rather than `config_root`. They
/// routinely list non-JS globs (`*.rs`, `*.py`) that ESLint never lints, so the
/// caller must apply this matcher only to the TypeScript family — a bare `*.rs`
/// line must not suppress Rust (or any other non-JS) source.
pub fn load_ignore_files(scan_root: &Path) -> Option<Gitignore> {
    let root = ignore_file_root(scan_root)?;
    let mut builder = GitignoreBuilder::new(&root);
    for name in IGNORE_FILES {
        // `add` reads the file (following a `.prettierignore` symlink) and
        // returns any parse error, which we skip rather than abort the matcher.
        let _ = builder.add(root.join(name));
    }
    builder.build().ok()
}

/// Nearest directory at or above `start` that holds an `.eslintignore` or
/// `.eslint-ignore` file, mirroring `config_root`'s upward search so scanning a
/// subtree still honors a repo-root ignore file.
fn ignore_file_root(start: &Path) -> Option<PathBuf> {
    let mut cur = Some(start);
    while let Some(dir) = cur {
        if IGNORE_FILES.iter().any(|f| dir.join(f).exists()) {
            return Some(dir.to_path_buf());
        }
        cur = dir.parent();
    }
    None
}

/// Nearest directory at or above `start` that holds an actual ESLint config.
/// A bare `package.json` is NOT a stop condition — in a monorepo the config
/// lives at the workspace root, above the nested per-package manifests, so the
/// search must walk past them. A `package.json` counts only if it carries an
/// `eslintConfig` block.
fn config_root(start: &Path) -> Option<PathBuf> {
    let mut cur = Some(start);
    while let Some(dir) = cur {
        let has_config = FLAT_CONFIGS.iter().any(|f| dir.join(f).exists())
            || dir.join(".eslintrc.json").exists()
            || dir.join(".eslintrc.js").exists()
            || dir.join(".eslintrc.cjs").exists()
            || dir.join(".eslintrc.yaml").exists()
            || dir.join(".eslintrc.yml").exists()
            || package_json_has_eslint_config(dir);
        if has_config {
            return Some(dir.to_path_buf());
        }
        cur = dir.parent();
    }
    None
}

fn package_json_has_eslint_config(dir: &Path) -> bool {
    std::fs::read_to_string(dir.join("package.json"))
        .ok()
        .and_then(|raw| serde_json::from_str::<serde_json::Value>(&raw).ok())
        .is_some_and(|v| v.get("eslintConfig").is_some())
}

fn collect_flat_config(root: &Path, out: &mut Vec<String>) {
    for name in FLAT_CONFIGS {
        if let Ok(src) = std::fs::read_to_string(root.join(name)) {
            collect_from_js(&src, Key::Ignores, out);
        }
    }
}

fn collect_eslintrc(root: &Path, out: &mut Vec<String>) {
    if let Ok(raw) = std::fs::read_to_string(root.join(".eslintrc.json"))
        && let Ok(value) = serde_json::from_str::<serde_json::Value>(&raw)
    {
        collect_json_ignore_patterns(&value, out);
    }
    for name in [".eslintrc.js", ".eslintrc.cjs"] {
        if let Ok(src) = std::fs::read_to_string(root.join(name)) {
            collect_from_js(&src, Key::IgnorePatterns, out);
        }
    }
    for name in [".eslintrc.yaml", ".eslintrc.yml"] {
        if let Ok(raw) = std::fs::read_to_string(root.join(name))
            && let Ok(value) = serde_yaml::from_str::<serde_yaml::Value>(&raw)
            && let Some(seq) = value.get("ignorePatterns").and_then(|x| x.as_sequence())
        {
            for item in seq {
                if let Some(s) = item.as_str() {
                    out.push(s.to_string());
                }
            }
        }
    }
}

fn collect_package_json(root: &Path, out: &mut Vec<String>) {
    if let Ok(raw) = std::fs::read_to_string(root.join("package.json"))
        && let Ok(value) = serde_json::from_str::<serde_json::Value>(&raw)
        && let Some(eslint_config) = value.get("eslintConfig")
    {
        collect_json_ignore_patterns(eslint_config, out);
    }
}

fn collect_json_ignore_patterns(value: &serde_json::Value, out: &mut Vec<String>) {
    if let Some(arr) = value.get("ignorePatterns").and_then(|x| x.as_array()) {
        for item in arr {
            if let Some(s) = item.as_str() {
                out.push(s.to_string());
            }
        }
    }
}

/// Which property holds the patterns and how to gate it.
#[derive(Clone, Copy)]
enum Key {
    /// flat config: collect only when `ignores` is the object's sole key
    /// (the documented "global ignores" form).
    Ignores,
    /// eslintrc: `ignorePatterns` is always global.
    IgnorePatterns,
}

/// Statically extract string-literal arrays bound to the target key from a
/// JS/TS config, via tree-sitter (no JS execution).
fn collect_from_js(source: &str, key: Key, out: &mut Vec<String>) {
    let mut parser = tree_sitter::Parser::new();
    let lang: tree_sitter::Language = tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into();
    if parser.set_language(&lang).is_err() {
        return;
    }
    let Some(tree) = parser.parse(source.as_bytes(), None) else {
        return;
    };
    let src = source.as_bytes();
    // Resolve same-file `const NAME = [literals]` so `...NAME` spreads inside an
    // ignore array expand — the common "ignore list as a const" pattern.
    let consts = collect_const_arrays(&tree, src);
    let target = match key {
        Key::Ignores => "ignores",
        Key::IgnorePatterns => "ignorePatterns",
    };
    walk_tree(&tree, |node| {
        if node.kind() != "pair" {
            return;
        }
        let Some(key_node) = node.child_by_field_name("key") else {
            return;
        };
        if key_name(key_node, src) != target {
            return;
        }
        if matches!(key, Key::Ignores) && !is_only_pair(node) {
            return;
        }
        let Some(value) = node.child_by_field_name("value") else {
            return;
        };
        if value.kind() == "array" {
            collect_array_strings(value, src, &consts, out);
        }
    });
}

/// Map of top-level `const NAME = [string-literals]` arrays, used to expand
/// `...NAME` spreads inside ignore arrays.
fn collect_const_arrays(tree: &tree_sitter::Tree, src: &[u8]) -> FxHashMap<String, Vec<String>> {
    let mut map = FxHashMap::default();
    walk_tree(tree, |node| {
        if node.kind() != "variable_declarator" {
            return;
        }
        let Some(name) = node.child_by_field_name("name") else {
            return;
        };
        if name.kind() != "identifier" {
            return;
        }
        let Some(array) = node.child_by_field_name("value").and_then(unwrap_array) else {
            return;
        };
        let mut strings = Vec::new();
        let mut cursor = array.walk();
        for child in array.named_children(&mut cursor) {
            if child.kind() == "string"
                && let Some(s) = string_value(child, src)
            {
                strings.push(s);
            }
        }
        if !strings.is_empty()
            && let Ok(ident) = name.utf8_text(src)
        {
            map.insert(ident.to_string(), strings);
        }
    });
    map
}

/// Unwrap `[...] as const` / `[...] satisfies T` down to the array node.
fn unwrap_array(node: tree_sitter::Node) -> Option<tree_sitter::Node> {
    match node.kind() {
        "array" => Some(node),
        "as_expression" | "satisfies_expression" => node.named_child(0).and_then(unwrap_array),
        _ => None,
    }
}

/// True when `pair`'s parent object holds no other key — flat config's
/// global-ignores form (`{ ignores: [...] }`).
fn is_only_pair(pair: tree_sitter::Node) -> bool {
    let Some(object) = pair.parent() else {
        return false;
    };
    let mut cursor = object.walk();
    object
        .named_children(&mut cursor)
        .filter(|c| c.kind() == "pair")
        .count()
        == 1
}

fn collect_array_strings(
    array: tree_sitter::Node,
    src: &[u8],
    consts: &FxHashMap<String, Vec<String>>,
    out: &mut Vec<String>,
) {
    let mut cursor = array.walk();
    for child in array.named_children(&mut cursor) {
        match child.kind() {
            "string" => {
                if let Some(s) = string_value(child, src) {
                    out.push(s);
                }
            }
            // `...NAME` where NAME is a same-file const array of literals.
            "spread_element" => {
                if let Some(ident) = child.named_child(0)
                    && ident.kind() == "identifier"
                    && let Ok(name) = ident.utf8_text(src)
                    && let Some(values) = consts.get(name)
                {
                    out.extend(values.iter().cloned());
                }
            }
            _ => {}
        }
    }
}

fn key_name(key: tree_sitter::Node, src: &[u8]) -> String {
    match key.kind() {
        "property_identifier" | "identifier" => key.utf8_text(src).unwrap_or("").to_string(),
        "string" => string_value(key, src).unwrap_or_default(),
        _ => String::new(),
    }
}

/// Inner text of a `string` node, without the surrounding quotes.
fn string_value(node: tree_sitter::Node, src: &[u8]) -> Option<String> {
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        if child.kind() == "string_fragment" {
            return child.utf8_text(src).ok().map(str::to_string);
        }
    }
    // Empty literals (`""`) carry no fragment child.
    let raw = node.utf8_text(src).ok()?;
    Some(
        raw.trim_matches(|c| c == '"' || c == '\'' || c == '`')
            .to_string(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flat(source: &str) -> Vec<String> {
        let mut out = Vec::new();
        collect_from_js(source, Key::Ignores, &mut out);
        out
    }

    fn rc(source: &str) -> Vec<String> {
        let mut out = Vec::new();
        collect_from_js(source, Key::IgnorePatterns, &mut out);
        out
    }

    #[test]
    fn flat_config_collects_global_ignores_only() {
        // First object is a global-ignores object (only `ignores`); the second
        // scopes `ignores` to its own `files`, so it is NOT a global ignore.
        let src = r#"
            export default [
              { ignores: ["**/dist", "**/*.gen.ts"] },
              { files: ["**/*.ts"], ignores: ["scoped.ts"] },
            ];
        "#;
        let got = flat(src);
        assert!(got.contains(&"**/dist".to_string()));
        assert!(got.contains(&"**/*.gen.ts".to_string()));
        assert!(!got.contains(&"scoped.ts".to_string()));
    }

    #[test]
    fn flat_config_ignores_dynamic_patterns_are_skipped() {
        // Spread of a runtime-read call: not statically resolvable.
        let src = "export default [{ ignores: [...readIgnore()] }];";
        assert!(flat(src).is_empty());
    }

    #[test]
    fn flat_config_expands_same_file_const_spread() {
        // The payload pattern: ignore list defined as a const, spread in.
        let src = r#"
            export const defaultIgnores = ["**/payload-types.ts", "**/dist/"];
            export default [
              { ignores: [...defaultIgnores, "examples/**"] },
            ];
        "#;
        let got = flat(src);
        assert!(got.contains(&"**/payload-types.ts".to_string()));
        assert!(got.contains(&"**/dist/".to_string()));
        assert!(got.contains(&"examples/**".to_string()));
    }

    #[test]
    fn eslintrc_js_collects_ignore_patterns() {
        let src = "module.exports = { ignorePatterns: [\"build/\", \"*.d.ts\"] };";
        let got = rc(src);
        assert_eq!(got, vec!["build/".to_string(), "*.d.ts".to_string()]);
    }
}
