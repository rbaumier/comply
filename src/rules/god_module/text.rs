//! god-module detection — cross-file check via `ProjectCtx::import_index()`.
//!
//! For each indexed TS/JS/TSX file, count how many other indexed files import
//! from it. If that count is both:
//!   - at least `min_importers` in absolute terms (defaults.toml: 10), AND
//!   - at least `threshold_percent` of the total indexed file count (30%),
//!     emit a diagnostic at line 1 of the offending module.
//!
//! The `min_importers` gate exists because in a project with 8 files every
//! shared helper would look like a god module by fraction alone — absolute
//! thresholding keeps the rule useful on realistic codebases only.
//!
//! Path handling: `ImportIndex` stores canonicalised absolute paths, while
//! `ctx.path` is whatever the user passed on the command line. We canonicalise
//! before looking up, and fall back to the raw path if canonicalize fails
//! (file deleted mid-run) — in that case the lookup misses silently.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

const RULE_ID: &str = "god-module";

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        // A pure re-export `index` file is a directory's public-API surface:
        // every consumer imports it, so high fan-in is expected and there is
        // nothing inside it to split. Exempt it before doing any index lookup.
        if is_pure_reexport_index(ctx.path, ctx.source) {
            return Vec::new();
        }

        let index = ctx.project.import_index();

        // Total indexed files = every file that made it through
        // `ImportIndex::build`. `iter_exports` enumerates exports per file
        // but the map contains an entry for every indexed TS/JS/TSX file
        // (exports vec may be empty), so the count is the denominator we want.
        let total_files = index.total_files();
        if total_files == 0 {
            // No cross-file index available (LSP / single-file run). The rule
            // has no signal to act on.
            return Vec::new();
        }

        let threshold_percent = ctx.config.threshold(RULE_ID, "threshold_percent", ctx.lang);
        let min_importers = ctx.config.threshold(RULE_ID, "min_importers", ctx.lang);

        let canon = index.canonical(ctx.path);
        let importer_count = index.importer_count(&canon);

        if importer_count < min_importers {
            return Vec::new();
        }

        // Integer math: fire when `importer_count / total_files > threshold / 100`.
        // Rearranged to avoid floats / rounding surprises:
        //   importer_count * 100 > threshold_percent * total_files
        if importer_count * 100 <= threshold_percent * total_files {
            return Vec::new();
        }

        // Percentage shown in the message is floor(importer_count * 100 / total).
        let percent = (importer_count * 100) / total_files;
        vec![Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: 1,
            column: 1,
            rule_id: RULE_ID.into(),
            message: format!(
                "imported by {importer_count}/{total_files} files ({percent}%). \
                 Consider splitting into smaller, focused modules."
            ),
            severity: Severity::Warning,
            span: None,
        }]
    }
}

/// True when `path`'s stem is `index` AND every meaningful statement in
/// `source` is a re-export passthrough. Such a file is a public-API barrel
/// with no logic of its own.
///
/// The check is content-shape based, not filename-only: an `index.ts` that
/// declares functions, variables or classes is NOT exempt — any statement that
/// is not a recognised re-export form disqualifies the file. Trivia ignored:
/// blank lines, `//` and `/* */` comments, and a leading `'use strict';`
/// directive. When a statement cannot be confidently classified as a re-export
/// the file is treated as NOT a shim, so the rule still fires (under-exempting
/// is safer than over-exempting).
fn is_pure_reexport_index(path: &std::path::Path, source: &str) -> bool {
    let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
        return false;
    };
    if stem != "index" {
        return false;
    }

    let stripped = strip_comments(source);
    let mut saw_reexport = false;
    for stmt in stripped.split(';') {
        let stmt = stmt.trim();
        if stmt.is_empty() || is_use_strict_directive(stmt) {
            continue;
        }
        if !is_reexport_statement(stmt) {
            return false;
        }
        saw_reexport = true;
    }
    // An empty file (no statements at all) is not a re-export surface.
    saw_reexport
}

/// Remove `//` line comments and `/* */` block comments, leaving everything
/// else (including string literals) untouched. String literals are tracked so
/// a `//` or `/*` inside a module path is not mistaken for a comment.
fn strip_comments(source: &str) -> String {
    let mut out = String::with_capacity(source.len());
    let bytes = source.as_bytes();
    let mut i = 0;
    let mut string_delim: Option<u8> = None;
    while i < bytes.len() {
        let b = bytes[i];
        if let Some(delim) = string_delim {
            out.push(b as char);
            if b == b'\\' && i + 1 < bytes.len() {
                out.push(bytes[i + 1] as char);
                i += 2;
                continue;
            }
            if b == delim {
                string_delim = None;
            }
            i += 1;
            continue;
        }
        match b {
            b'\'' | b'"' | b'`' => {
                string_delim = Some(b);
                out.push(b as char);
                i += 1;
            }
            b'/' if i + 1 < bytes.len() && bytes[i + 1] == b'/' => {
                while i < bytes.len() && bytes[i] != b'\n' {
                    i += 1;
                }
            }
            b'/' if i + 1 < bytes.len() && bytes[i + 1] == b'*' => {
                i += 2;
                while i + 1 < bytes.len() && !(bytes[i] == b'*' && bytes[i + 1] == b'/') {
                    i += 1;
                }
                i += 2;
            }
            _ => {
                out.push(b as char);
                i += 1;
            }
        }
    }
    out
}

/// A `'use strict';` / `"use strict";` directive (the `;` is already consumed
/// by the statement split, so `stmt` is just the string literal).
fn is_use_strict_directive(stmt: &str) -> bool {
    stmt == "'use strict'" || stmt == "\"use strict\""
}

/// A single re-export passthrough statement (comment-stripped, `;`-trimmed):
///   - `module.exports = require('...')` with optional `.foo` member access
///   - `export * from '...'`
///   - `export { ... } from '...'`  (includes `export { default } from '...'`)
fn is_reexport_statement(stmt: &str) -> bool {
    if let Some(rhs) = stmt.strip_prefix("module.exports") {
        let rhs = rhs.trim_start();
        let Some(rhs) = rhs.strip_prefix('=') else {
            return false;
        };
        let rhs = rhs.trim_start();
        return is_require_passthrough(rhs);
    }

    let rest = match stmt.strip_prefix("export") {
        Some(rest) => rest.trim_start(),
        None => return false,
    };

    if let Some(after_star) = rest.strip_prefix('*') {
        // `export * from '...'` or `export * as ns from '...'`
        return has_from_clause(after_star);
    }
    if rest.starts_with('{') {
        // `export { ... } from '...'` — must re-export FROM another module,
        // not a local `export { foo }` (which exposes in-file declarations).
        return has_from_clause(rest);
    }
    false
}

/// True when `rhs` is `require('...')` optionally followed by `.member` access,
/// e.g. `require('./lib/express')` or `require('./lib/x').default`.
fn is_require_passthrough(rhs: &str) -> bool {
    let Some(after) = rhs.strip_prefix("require") else {
        return false;
    };
    let after = after.trim_start();
    let Some(after) = after.strip_prefix('(') else {
        return false;
    };
    let Some(close) = after.find(')') else {
        return false;
    };
    let arg = after[..close].trim();
    if !is_string_literal(arg) {
        return false;
    }
    // Anything after the closing paren must be plain `.member` accesses only.
    let tail = after[close + 1..].trim();
    tail.is_empty() || is_member_access_tail(tail)
}

/// True when `rest` contains a `from '...'` clause whose specifier is a string
/// literal (the source module of a re-export).
fn has_from_clause(rest: &str) -> bool {
    let Some(idx) = rest.find("from") else {
        return false;
    };
    let spec = rest[idx + "from".len()..].trim();
    is_string_literal(spec)
}

/// A `.foo.bar` chain of identifier member accesses and nothing else.
fn is_member_access_tail(tail: &str) -> bool {
    let Some(rest) = tail.strip_prefix('.') else {
        return false;
    };
    !rest.is_empty()
        && rest
            .split('.')
            .all(|seg| !seg.is_empty() && seg.chars().all(|c| c.is_alphanumeric() || c == '_'))
}

/// A quoted string literal (`'...'`, `"..."`, or `` `...` ``) with no embedded
/// quote of the same kind — the conservative shape of a module specifier.
fn is_string_literal(s: &str) -> bool {
    let bytes = s.as_bytes();
    if bytes.len() < 2 {
        return false;
    }
    let first = bytes[0];
    if first != b'\'' && first != b'"' && first != b'`' {
        return false;
    }
    bytes[bytes.len() - 1] == first && !s[1..s.len() - 1].contains(first as char)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::files::{Language, SourceFile};
    use crate::project::ProjectCtx;
    use crate::rules::file_ctx::FileCtx;
    use std::fs;
    use std::path::PathBuf;
    use tempfile::TempDir;

    /// Build a project on disk with N files, all importing from `hub.ts`,
    /// plus `extra_files` untouched files to control the ratio.
    fn run_on_project(files: &[(&str, &str)], target_rel: &str) -> (TempDir, Vec<Diagnostic>) {
        let dir = TempDir::new().unwrap();
        let mut source_files: Vec<SourceFile> = Vec::new();
        for (rel, content) in files {
            let p = dir.path().join(rel);
            if let Some(parent) = p.parent() {
                fs::create_dir_all(parent).unwrap();
            }
            fs::write(&p, content).unwrap();
            let lang = Language::from_path(&p).unwrap();
            source_files.push(SourceFile {
                path: p,
                language: lang,
            });
        }
        let refs: Vec<&SourceFile> = source_files.iter().collect();
        let config = Config::default();
        let project = ProjectCtx::load(&refs, &config);

        let target_path: PathBuf = dir.path().join(target_rel);
        let source = fs::read_to_string(&target_path).unwrap();
        let file_ctx = FileCtx::empty();
        let ctx = CheckCtx {
            path: &target_path,
            path_arc: std::sync::Arc::from(target_path.as_path()),
            source: &source,
            config: &config,
            project: &project,
            file: &file_ctx, lang: crate::files::Language::TypeScript,
        };
        let diags = Check.check(&ctx);
        (dir, diags)
    }

    #[test]
    fn flags_hub_imported_by_more_than_threshold() {
        // 1 hub + 12 importers = 13 files. Importers/total = 12/13 ~= 92%,
        // well above 30% and well above min_importers = 10.
        let mut files: Vec<(String, String)> = Vec::new();
        files.push(("hub.ts".to_string(), "export const x = 1;\n".to_string()));
        for i in 0..12 {
            files.push((
                format!("a{i}.ts"),
                "import { x } from './hub';\n".to_string(),
            ));
        }
        let borrowed: Vec<(&str, &str)> = files
            .iter()
            .map(|(a, b)| (a.as_str(), b.as_str()))
            .collect();
        let (_dir, diags) = run_on_project(&borrowed, "hub.ts");
        assert_eq!(diags.len(), 1, "expected one god-module diagnostic");
        assert_eq!(diags[0].rule_id, "god-module");
    }

    #[test]
    fn allows_module_below_threshold_percent() {
        // 1 hub + 2 importers out of 20 files = 10%, below 30%.
        let mut files: Vec<(String, String)> = vec![
            ("hub.ts".into(), "export const x = 1;\n".into()),
            ("a0.ts".into(), "import { x } from './hub';\n".into()),
            ("a1.ts".into(), "import { x } from './hub';\n".into()),
        ];
        for i in 0..17 {
            files.push((format!("b{i}.ts"), "export const y = 1;\n".into()));
        }
        let borrowed: Vec<(&str, &str)> = files
            .iter()
            .map(|(a, b)| (a.as_str(), b.as_str()))
            .collect();
        let (_dir, diags) = run_on_project(&borrowed, "hub.ts");
        assert!(diags.is_empty(), "ratio 2/20 = 10% < 30% should not fire");
    }

    #[test]
    fn allows_module_below_min_importers_even_if_ratio_high() {
        // 1 hub + 3 importers in a 4-file project = 75% ratio but only 3
        // absolute importers — below the default `min_importers` = 10.
        let files: Vec<(&str, &str)> = vec![
            ("hub.ts", "export const x = 1;"),
            ("a.ts", "import { x } from './hub';"),
            ("b.ts", "import { x } from './hub';"),
            ("c.ts", "import { x } from './hub';"),
        ];
        let (_dir, diags) = run_on_project(&files, "hub.ts");
        assert!(
            diags.is_empty(),
            "absolute importer count < min_importers gates the rule"
        );
    }

    #[test]
    fn ignores_file_with_no_importers() {
        // Standalone file, no importers. Must stay silent.
        let files: Vec<(&str, &str)> = vec![
            ("hub.ts", "export const x = 1;"),
            ("other.ts", "export const y = 2;"),
        ];
        let (_dir, diags) = run_on_project(&files, "hub.ts");
        assert!(diags.is_empty());
    }

    /// Build N importers of `target_rel` so the target is high fan-in, then run
    /// the rule on it. Mirrors `run_on_project` but lets the target carry an
    /// arbitrary body (e.g. a re-export shim).
    fn run_high_fanin(target_rel: &str, target_body: &str) -> (TempDir, Vec<Diagnostic>) {
        let mut files: Vec<(String, String)> = vec![(target_rel.to_string(), target_body.to_string())];
        let target_spec = format!(
            "./{}",
            std::path::Path::new(target_rel)
                .file_stem()
                .unwrap()
                .to_str()
                .unwrap()
        );
        for i in 0..12 {
            files.push((
                format!("a{i}.ts"),
                format!("import {{ x }} from '{target_spec}';\n"),
            ));
        }
        let borrowed: Vec<(&str, &str)> = files
            .iter()
            .map(|(a, b)| (a.as_str(), b.as_str()))
            .collect();
        run_on_project(&borrowed, target_rel)
    }

    #[test]
    fn exempts_commonjs_reexport_index_entry_point() {
        // expressjs/express index.js: a `'use strict'` directive + a single
        // `module.exports = require(...)` passthrough. High fan-in, no logic.
        let body = "'use strict';\nmodule.exports = require('./lib/express');\n";
        let (_dir, diags) = run_high_fanin("index.js", body);
        assert!(
            diags.is_empty(),
            "pure CommonJS re-export index must not be flagged, got {diags:?}"
        );
    }

    #[test]
    fn exempts_esm_barrel_index_entry_point() {
        // A pure ESM barrel: only `export * from '...'` lines.
        let body = "export * from './a';\nexport * from './b';\n";
        let (_dir, diags) = run_high_fanin("index.ts", body);
        assert!(
            diags.is_empty(),
            "pure ESM re-export barrel must not be flagged, got {diags:?}"
        );
    }

    #[test]
    fn still_flags_index_with_real_logic() {
        // Named `index.ts` but carries real logic (a function + property
        // accumulation), so it is NOT a transparent shim and must still fire.
        let body = "export function build() { return 1; }\n\
                    export const registry = {};\n\
                    registry.foo = build();\n";
        let (_dir, diags) = run_high_fanin("index.ts", body);
        assert_eq!(
            diags.len(),
            1,
            "index.ts with logic must still be flagged, got {diags:?}"
        );
    }

    #[test]
    fn still_flags_non_index_high_fanin_module() {
        // A non-`index` hub with re-export-shaped content must still fire — the
        // exemption is gated on the `index` stem (a directory's public API).
        let body = "export * from './a';\n";
        let (_dir, diags) = run_high_fanin("hub.ts", body);
        assert_eq!(
            diags.len(),
            1,
            "non-index high-fan-in module must still be flagged, got {diags:?}"
        );
    }

    #[test]
    fn reexport_index_with_local_export_still_flags() {
        // `index.ts` that re-exports AND declares a local export is not a pure
        // shim — the local `export const` is real surface area.
        let body = "export * from './a';\nexport const version = '1.0.0';\n";
        let (_dir, diags) = run_high_fanin("index.ts", body);
        assert_eq!(
            diags.len(),
            1,
            "index with a local declaration must still be flagged, got {diags:?}"
        );
    }
}
