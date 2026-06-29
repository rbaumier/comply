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

        // A constants-only module (error sentinels, enums, status codes,
        // lookup tables) has exactly one responsibility — it holds values — so
        // it cannot accumulate the unrelated responsibilities the rule targets.
        // Its high fan-in reflects a universally-needed domain concept, not a
        // centralisation smell, and "split into smaller modules" is inapplicable
        // because there is no behaviour to extract.
        if is_constants_only_module(ctx.source) {
            return Vec::new();
        }

        // A types-only module declares only TypeScript type surface (interfaces,
        // type aliases, type-only re-exports) and carries no runtime behaviour.
        // Like a constants-only module it has exactly one responsibility —
        // declaring the module's type surface — so its high fan-in reflects
        // widespread *use* of those types, not centralised logic. There is no
        // behaviour to extract, so "split into smaller modules" does not apply.
        if is_types_only_module(ctx.source) {
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

/// True when `source` is a module that exports only constant values and
/// declares no behaviour — error sentinels, enums, status-code tables, lookup
/// maps. Such a module has a single responsibility (holding values) regardless
/// of how many files import it, so its fan-in is never a centralisation smell.
///
/// The discriminator is the *absence of behaviour*, not the file's name or
/// path: after stripping comments and string/template literals, the module
/// must export something yet contain no function or class declaration, no
/// arrow function, and no method/getter body (`) {`). A `new Error('…')` value
/// is fine — it is a constructed constant, not a declared function. Any
/// behaviour marker disqualifies the file, so a module that mixes constants
/// with functions or classes is still flagged. Under-exempting is safer than
/// over-exempting: a file with no recognisable export is treated as not
/// constants-only.
fn is_constants_only_module(source: &str) -> bool {
    let code = strip_strings(&strip_comments(source));

    // The module must bind at least one constant VALUE export. A re-export
    // barrel (`export * from`, `export { … } from`) is handled by the
    // `is_pure_reexport_index` exemption and is not a constants-only module —
    // disqualify it so a non-`index` barrel still flags.
    if !has_value_export(&code) {
        return false;
    }

    // A `from` clause means an `import … from` or `export … from` passthrough —
    // coupling / re-export surface area, not a self-contained constants module.
    // Pure re-export barrels are handled by `is_pure_reexport_index`.
    if has_word(&code, "from") {
        return false;
    }

    !has_behaviour_marker(&code)
}

/// True when `source` is a module that declares only TypeScript type surface —
/// `interface` declarations, `type` aliases, and type-only re-exports — and
/// carries no runtime behaviour. Such a module has a single responsibility
/// (declaring the module's type surface) regardless of how many files import
/// it, so its fan-in is never a centralisation smell.
///
/// The discriminator is structural, not name/path based. On the comment- and
/// string-stripped source (same input as `is_constants_only_module`):
///   1. no runtime value export is present (`export const/let/var/default/enum`,
///      `module.exports`, `exports.`);
///   2. no behaviour marker is present (`has_behaviour_marker`);
///   3. at least one type-level export is present.
///
/// Condition 2 guarantees the safety property: a module carrying any behaviour
/// can never match, so a genuine behavioural god-module is never exempted. A
/// types file that uses arrow-function *type* syntax (`=>`) therefore stays
/// flagged — the conservative trade-off, identical to `is_constants_only_module`.
fn is_types_only_module(source: &str) -> bool {
    let code = strip_strings(&strip_comments(source));

    // Any runtime value export means the module is not purely types.
    if has_value_export(&code) {
        return false;
    }

    if has_behaviour_marker(&code) {
        return false;
    }

    // At least one actual type declaration/export. `export type` covers both
    // type aliases and type-only re-exports (`export type { … } from`);
    // `export interface` is subsumed by the bare `interface` whole-word check.
    // A bare `export *` is deliberately NOT accepted on its own — it re-exports
    // runtime values too, so it is indistinguishable from a value barrel, and a
    // non-`index` barrel must still flag (`still_flags_non_index_high_fanin_module`).
    code.contains("export type") || has_word(&code, "interface")
}

/// True when `code` (already comment- and string-stripped) contains any marker
/// of runtime behaviour:
///   - `function` / `class`  : declared or expression function/class
///   - `=>`                  : arrow function value
///   - `) {` / `){`          : a function/method/getter body (object and
///                             array literals never contain a `)`-then-`{`)
///
/// Shared by `is_constants_only_module` and `is_types_only_module` so both gate
/// on one definition of "behaviour".
fn has_behaviour_marker(code: &str) -> bool {
    has_word(code, "function")
        || has_word(code, "class")
        || code.contains("=>")
        || code.contains(") {")
        || code.contains("){")
}

/// True when `code` (already comment- and string-stripped) binds at least one
/// runtime value export or CommonJS export: `export const/let/var/default/enum`,
/// `module.exports`, or `exports.<member>`. Type-level exports (`export type`,
/// `export interface`) are not value exports. Shared by `is_constants_only_module`
/// and `is_types_only_module`.
fn has_value_export(code: &str) -> bool {
    code.contains("export const")
        || code.contains("export let")
        || code.contains("export var")
        || code.contains("export default")
        || code.contains("export enum")
        || code.contains("module.exports")
        || code.contains("exports.")
}

/// True when `needle` appears in `code` as a whole word — not as a substring of
/// a longer identifier (so `classify` / `functional` don't trip the `class` /
/// `function` markers).
fn has_word(code: &str, needle: &str) -> bool {
    let mut from = 0;
    while let Some(rel) = code[from..].find(needle) {
        let start = from + rel;
        let end = start + needle.len();
        let before_ok = start == 0
            || !is_ident_char(code.as_bytes()[start - 1]);
        let after_ok = end == code.len() || !is_ident_char(code.as_bytes()[end]);
        if before_ok && after_ok {
            return true;
        }
        from = start + 1;
    }
    false
}

fn is_ident_char(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b == b'$'
}

/// Replace the contents of every `'…'`, `"…"`, and `` `…` `` literal with
/// spaces, leaving the surrounding code intact. Used so behaviour markers
/// (`function`, `=>`, `) {`) inside a string value cannot trip the
/// constants-only check. Assumes comments are already stripped.
fn strip_strings(code: &str) -> String {
    let mut out = String::with_capacity(code.len());
    let bytes = code.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if b == b'\'' || b == b'"' || b == b'`' {
            out.push(b as char);
            i += 1;
            while i < bytes.len() {
                let c = bytes[i];
                if c == b'\\' && i + 1 < bytes.len() {
                    out.push(' ');
                    out.push(' ');
                    i += 2;
                    continue;
                }
                if c == b {
                    out.push(c as char);
                    i += 1;
                    break;
                }
                // Preserve newlines so line structure is unchanged; blank the rest.
                out.push(if c == b'\n' { '\n' } else { ' ' });
                i += 1;
            }
            continue;
        }
        out.push(b as char);
        i += 1;
    }
    out
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
        // well above 30% and well above min_importers = 10. The hub declares a
        // function so it is behaviour-bearing, not a constants-only module.
        let mut files: Vec<(String, String)> = Vec::new();
        files.push((
            "hub.ts".to_string(),
            "export function x() { return 1; }\n".to_string(),
        ));
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

    #[test]
    fn exempts_const_error_sentinels_module() {
        // formulajs/formulajs src/utils/error.js: only `export const X = new
        // Error('…')` bindings. High fan-in (every formula imports it) but the
        // module is constants-only, so it must not be flagged. Closes #6104.
        let body = "export const nil = new Error('#NULL!')\n\
                    export const div0 = new Error('#DIV/0!')\n\
                    export const value = new Error('#VALUE!')\n";
        let (_dir, diags) = run_high_fanin("error.js", body);
        assert!(
            diags.is_empty(),
            "constants-only error-sentinel module must not be flagged, got {diags:?}"
        );
    }

    #[test]
    fn exempts_default_export_constants_object() {
        // A single default-exported constants object (no functions/classes).
        let body = "export default {\n\
                    nil: '#NULL!',\n\
                    div0: '#DIV/0!',\n\
                    value: '#VALUE!',\n\
                    }\n";
        let (_dir, diags) = run_high_fanin("error.js", body);
        assert!(
            diags.is_empty(),
            "constants object default export must not be flagged, got {diags:?}"
        );
    }

    #[test]
    fn exempts_ts_enum_module() {
        // A TS enum module — pure value definitions, no behaviour.
        let body = "export enum Status {\n  Ok = 200,\n  NotFound = 404,\n}\n";
        let (_dir, diags) = run_high_fanin("status.ts", body);
        assert!(
            diags.is_empty(),
            "enum-only module must not be flagged, got {diags:?}"
        );
    }

    #[test]
    fn still_flags_module_declaring_functions() {
        // High fan-in module that declares real behaviour — still flagged.
        let body = "export function add(a, b) {\n  return a + b;\n}\n\
                    export const ZERO = 0;\n";
        let (_dir, diags) = run_high_fanin("math.ts", body);
        assert_eq!(
            diags.len(),
            1,
            "module declaring a function must still be flagged, got {diags:?}"
        );
    }

    #[test]
    fn still_flags_module_with_arrow_function_export() {
        // A constant bound to an arrow function is behaviour, not a value.
        let body = "export const handler = (req, res) => res.end();\n\
                    export const NAME = 'x';\n";
        let (_dir, diags) = run_high_fanin("handler.ts", body);
        assert_eq!(
            diags.len(),
            1,
            "module exporting an arrow function must still be flagged, got {diags:?}"
        );
    }

    #[test]
    fn still_flags_constants_object_with_method() {
        // An object that mixes constant fields with a method has behaviour.
        let body = "export default {\n  nil: '#NULL!',\n  format(code) {\n    return code;\n  },\n}\n";
        let (_dir, diags) = run_high_fanin("error.js", body);
        assert_eq!(
            diags.len(),
            1,
            "constants object with a method must still be flagged, got {diags:?}"
        );
    }

    #[test]
    fn still_flags_module_declaring_class() {
        let body = "export class Money {\n  constructor() {}\n}\n";
        let (_dir, diags) = run_high_fanin("money.ts", body);
        assert_eq!(
            diags.len(),
            1,
            "module declaring a class must still be flagged, got {diags:?}"
        );
    }

    #[test]
    fn behaviour_marker_inside_string_does_not_exempt_failure() {
        // A behaviour marker that lives only inside a string literal must not
        // wrongly KEEP a real-behaviour module flagged, nor wrongly exempt a
        // constants module: a const whose string value contains `function`/`=>`
        // is still constants-only and must be exempt.
        let body = "export const TEMPLATE = 'const f = () => {}';\n\
                    export const HINT = 'use function keyword';\n";
        let (_dir, diags) = run_high_fanin("templates.ts", body);
        assert!(
            diags.is_empty(),
            "behaviour markers inside string values must not flag a constants module, got {diags:?}"
        );
    }

    #[test]
    fn ts_function_type_annotation_stays_flagged() {
        // A constant whose TS *type annotation* contains `=>` is kept flagged.
        // The text-shape check cannot distinguish an arrow type from an arrow
        // value, so it under-exempts here — safer than wrongly exempting a real
        // behaviour module. This test pins that intentional decision.
        let body = "export const noop: () => void = NOOP;\n";
        let (_dir, diags) = run_high_fanin("noop.ts", body);
        assert_eq!(
            diags.len(),
            1,
            "TS arrow-type annotation is conservatively kept flagged, got {diags:?}"
        );
    }

    #[test]
    fn exempts_types_only_module() {
        // unjs/magicast src/types.ts: only type declarations, type aliases and
        // type-only re-exports — no runtime value, no behaviour. High fan-in
        // reflects use of the type surface, not centralised logic. Closes #6666.
        let body = "import type { Program } from \"@babel/types\";\n\
                    import { Options as ParseOptions } from \"recast\";\n\
                    import { CodeFormatOptions } from \"./format\";\n\
                    \n\
                    export type { Node as ASTNode } from \"@babel/types\";\n\
                    export * from \"./proxy/types\";\n\
                    \n\
                    export interface Loc {\n\
                      start?: { line?: number; column?: number };\n\
                    }\n\
                    export interface Token { type: string; value: string; loc?: Loc; }\n\
                    export interface ParsedFileNode { program: Program; source: string; }\n\
                    export type ProxifiedValue = ParsedFileNode | Token;\n";
        let (_dir, diags) = run_high_fanin("types.ts", body);
        assert!(
            diags.is_empty(),
            "types-only module must not be flagged, got {diags:?}"
        );
    }

    #[test]
    fn still_flags_behavioural_module_high_fanin() {
        // Functions and classes are behaviour: the types-only exemption cannot
        // match (behaviour markers present), so a real hub still fires.
        let body = "export function load() { return 1; }\n\
                    export class Store {}\n";
        let (_dir, diags) = run_high_fanin("store.ts", body);
        assert_eq!(
            diags.len(),
            1,
            "behavioural module must still be flagged, got {diags:?}"
        );
    }

    #[test]
    fn still_flags_module_mixing_types_and_function() {
        // Declares a type AND exports a function: a behaviour marker is present,
        // so the types-only exemption does not match and the module stays flagged.
        let body = "export interface Config { name: string; }\n\
                    export function load(c: Config): Config { return c; }\n";
        let (_dir, diags) = run_high_fanin("config.ts", body);
        assert_eq!(
            diags.len(),
            1,
            "module mixing types with a function must still be flagged, got {diags:?}"
        );
    }
}
