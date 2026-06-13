//! Module Federation remote-name extraction.
//!
//! Webpack / Rspack / Rsbuild / Vite Module Federation plugins declare a
//! `remotes` map whose keys become importable module namespaces resolved at
//! runtime (`import X from "remote/Exposed"`). Those namespaces are never npm
//! packages and must not appear in `package.json`, so `no-implicit-deps` reads
//! the nearest bundler config to learn which root names are federated remotes.

use std::collections::HashSet;
use std::path::Path;

/// Bundler config files that may host a Module Federation `remotes` map.
const CONFIG_FILES: &[&str] = &[
    "rsbuild.config.ts",
    "rsbuild.config.js",
    "rsbuild.config.mts",
    "rsbuild.config.mjs",
    "rspack.config.ts",
    "rspack.config.js",
    "rspack.config.mts",
    "rspack.config.mjs",
    "webpack.config.ts",
    "webpack.config.js",
    "webpack.config.mts",
    "webpack.config.mjs",
    "webpack.config.cjs",
    "vite.config.ts",
    "vite.config.js",
    "vite.config.mts",
    "vite.config.mjs",
    "module-federation.config.ts",
    "module-federation.config.js",
];

/// Collect Module Federation remote names declared in any bundler config
/// between `importer`'s directory and `stop_at` (inclusive). Walks no further
/// than `stop_at` so the scan never escapes the project root.
pub(super) fn remote_names(importer: &Path, stop_at: Option<&Path>) -> HashSet<String> {
    let mut names = HashSet::new();
    let mut dir = importer.parent();
    while let Some(d) = dir {
        for file in CONFIG_FILES {
            let path = d.join(file);
            if let Ok(raw) = std::fs::read_to_string(&path) {
                collect_remote_names(&raw, &mut names);
            }
        }
        if stop_at == Some(d) {
            break;
        }
        dir = d.parent();
    }
    names
}

/// Extract the keys of every `remotes: { ... }` object literal in `source`.
/// Keys are either bare JS identifiers (`remote:`) or quoted strings
/// (`"remote-app":`). Nested braces inside values are skipped so the scan
/// stops at the matching close brace of the `remotes` object.
fn collect_remote_names(source: &str, out: &mut HashSet<String>) {
    let bytes = source.as_bytes();
    let mut search_from = 0;
    while let Some(rel) = source[search_from..].find("remotes") {
        let kw_start = search_from + rel;
        let kw_end = kw_start + "remotes".len();
        search_from = kw_end;

        // `remotes` must be a standalone property key, not a substring of a
        // larger identifier (e.g. `myRemotes`, `remotesList`).
        if kw_start > 0 && is_ident_char(bytes[kw_start - 1]) {
            continue;
        }
        if kw_end < bytes.len() && is_ident_char(bytes[kw_end]) {
            continue;
        }

        // Require `:` then `{` (whitespace allowed between each token).
        let Some(colon) = next_non_ws(bytes, kw_end) else {
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

        collect_object_keys(source, open, out);
    }
}

/// Walk the object literal whose `{` is at `open`, recording each top-level
/// key. Tracks brace depth so keys of nested objects (remote option blocks)
/// are not mistaken for remote names; ignores `:` / `{` inside strings.
fn collect_object_keys(source: &str, open: usize, out: &mut HashSet<String>) {
    let bytes = source.as_bytes();
    let mut i = open + 1;
    let mut depth = 1usize;
    let mut expect_key = true;
    while i < bytes.len() {
        match bytes[i] {
            b' ' | b'\t' | b'\r' | b'\n' => i += 1,
            b',' if depth == 1 => {
                expect_key = true;
                i += 1;
            }
            b',' => i += 1,
            b'{' | b'[' | b'(' => {
                depth += 1;
                expect_key = false;
                i += 1;
            }
            b'}' | b']' | b')' => {
                depth -= 1;
                if depth == 0 {
                    return;
                }
                i += 1;
            }
            b'"' | b'\'' if depth == 1 && expect_key => {
                let quote = bytes[i];
                let key_start = i + 1;
                let mut j = key_start;
                while j < bytes.len() && bytes[j] != quote {
                    if bytes[j] == b'\\' {
                        j += 1;
                    }
                    j += 1;
                }
                if let Some(key) = source.get(key_start..j) {
                    out.insert(key.to_string());
                }
                expect_key = false;
                i = j + 1;
            }
            b'"' | b'\'' => {
                // String value: skip to its close quote.
                let quote = bytes[i];
                let mut j = i + 1;
                while j < bytes.len() && bytes[j] != quote {
                    if bytes[j] == b'\\' {
                        j += 1;
                    }
                    j += 1;
                }
                i = j + 1;
            }
            b':' if depth == 1 => {
                expect_key = false;
                i += 1;
            }
            c if depth == 1 && expect_key && is_ident_start(c) => {
                let key_start = i;
                let mut j = i + 1;
                while j < bytes.len() && is_ident_char(bytes[j]) {
                    j += 1;
                }
                if let Some(key) = source.get(key_start..j) {
                    out.insert(key.to_string());
                }
                expect_key = false;
                i = j;
            }
            _ => i += 1,
        }
    }
}

fn next_non_ws(bytes: &[u8], from: usize) -> Option<usize> {
    let mut i = from;
    while i < bytes.len() && matches!(bytes[i], b' ' | b'\t' | b'\r' | b'\n') {
        i += 1;
    }
    (i < bytes.len()).then_some(i)
}

fn is_ident_start(c: u8) -> bool {
    c.is_ascii_alphabetic() || c == b'_' || c == b'$'
}

fn is_ident_char(c: u8) -> bool {
    c.is_ascii_alphanumeric() || c == b'_' || c == b'$'
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(source: &str) -> HashSet<String> {
        let mut out = HashSet::new();
        collect_remote_names(source, &mut out);
        out
    }

    #[test]
    fn extracts_bare_identifier_key() {
        let src = r#"pluginModuleFederation({ name: "host", remotes: { remote: "remote@http://x/mf.json" } })"#;
        let got = names(src);
        assert!(got.contains("remote"), "got {got:?}");
        assert!(!got.contains("name"));
    }

    #[test]
    fn extracts_quoted_string_key() {
        let src = r#"remotes: { "remote-app": "remote-app@http://x/mf.json", checkout: "checkout@http://y/mf.json" }"#;
        let got = names(src);
        assert!(got.contains("remote-app"), "got {got:?}");
        assert!(got.contains("checkout"), "got {got:?}");
    }

    #[test]
    fn ignores_nested_option_keys() {
        let src = r#"remotes: { remote: { external: "remote@http://x/mf.json", format: "var" } }"#;
        let got = names(src);
        assert!(got.contains("remote"), "got {got:?}");
        assert!(!got.contains("external"), "got {got:?}");
        assert!(!got.contains("format"), "got {got:?}");
    }

    #[test]
    fn ignores_substring_of_larger_identifier() {
        let src = r#"const myRemotes = { foo: 1 }; remotesList: [ "a" ]"#;
        assert!(names(src).is_empty(), "got {:?}", names(src));
    }

    #[test]
    fn handles_multiple_remotes_blocks() {
        let src = r#"
            pluginModuleFederation({ remotes: { a: "a@u" } });
            pluginModuleFederation({ remotes: { b: "b@u" } });
        "#;
        let got = names(src);
        assert!(got.contains("a") && got.contains("b"), "got {got:?}");
    }

    #[test]
    fn no_remotes_yields_empty() {
        let src = r#"export default defineConfig({ plugins: [pluginReact()] })"#;
        assert!(names(src).is_empty());
    }
}
