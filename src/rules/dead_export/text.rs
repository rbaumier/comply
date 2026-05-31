//! dead-export detection — walk every export in the current file and verify
//! it has at least one linked importer in the index.
//!
//! Skips:
//!   - Test files (`*.test.*`, `*.spec.*`, `tests/`, `__tests__/`) — these
//!     may legitimately export fixtures used only internally.
//!   - Entry points (`main.*`, `index.*` at the project root) — they are the
//!     consumer, not the consumed, and aren't imported by convention.
//!   - Star re-exports (`export * from './m'`) — the re-export doesn't carry
//!     a specific name to link against; it's a barrel, not a dead symbol.
//!   - Reusable UI library directories (`components/ui/`, `lib/ui/`) — these
//!     hold drop-in components (shadcn convention) that are installed for
//!     future use; flagging them every time a developer adds one before its
//!     first import is pure noise.
//!   - Generated files (containing a `// @generated` or `/* @generated */`
//!     header in the first ~40 lines) — code generators emit a fixed export
//!     surface that callers may pick from gradually.
//!
//! False-positive guards:
//!   - If any file imports the current module via a namespace import
//!     (`import * as ns from './m'`), `symbol_usages` is intentionally not
//!     populated for individual names. In that case every export on the
//!     module is treated as live — we can't tell from the index alone which
//!     specific names `ns.*` accesses touch.
//!   - `export default` is matched against the `"default"` usage key.
//!   - Exported types/interfaces that parameterize the signature of another
//!     exported function in the same file are kept — callers consume them
//!     structurally (by passing an object literal to that function) without
//!     ever importing the type name.
//!   - Exports referenced anywhere else in the same file (schema chains like
//!     `BaseSchema.extend(...)`, `z.infer<typeof BaseSchema>`, composition
//!     into another exported value) are kept. The base name is consumed
//!     in-file; its derived form is what callers import.

use crate::diagnostic::{Diagnostic, Severity};
use crate::parsing::ts_language_for;
use crate::project::import_index::{ExportKind, ImportKind};
use crate::rules::backend::{CheckCtx, TextCheck};
use crate::rules::path_utils::{is_config_file, is_framework_entry_point};
use crate::rules::walker::walk_tree;
use std::collections::HashSet;
use std::path::Path;

const RULE_ID: &str = "dead-export";

/// Path segments that mark a directory as a reusable UI component library.
/// Matched against the canonicalised path with forward-slash separators.
const UI_LIBRARY_DIRS: &[&str] = &["/components/ui/", "/lib/ui/", "/src/components/ui/"];

fn is_in_ui_library(path: &Path) -> bool {
    let normalised = path.to_string_lossy().replace('\\', "/");
    UI_LIBRARY_DIRS.iter().any(|seg| normalised.contains(seg))
}

const FIXTURE_DIRS: &[&str] = &["__testfixtures__", "__fixtures__", "fixtures", "test-fixtures"];

fn is_in_fixture_dir(path: &Path) -> bool {
    let normalised = path.to_string_lossy().replace('\\', "/");
    FIXTURE_DIRS.iter().any(|seg| normalised.contains(seg))
}

/// True if the source carries a `@generated` marker in its leading comments.
/// Only scans the first 2KB to keep the cost bounded; generators always emit
/// the marker at the top of the file.
fn is_generated(source: &str) -> bool {
    let mut end = source.len().min(2048);
    while !source.is_char_boundary(end) {
        end -= 1;
    }
    source[..end].contains("@generated")
}

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        if ctx.file.path_segments.in_test_dir {
            return Vec::new();
        }
        if is_config_file(ctx.path) {
            return Vec::new();
        }
        if is_entry_point(ctx.path, ctx.project.project_root.as_deref()) {
            return Vec::new();
        }
        if is_in_ui_library(ctx.path) {
            return Vec::new();
        }
        if is_generated(ctx.source) {
            return Vec::new();
        }
        if is_in_fixture_dir(ctx.path) {
            return Vec::new();
        }
        if ctx.project.nearest_package_json(ctx.path).is_some_and(|pkg| {
            pkg.is_library || is_script_entry_point(ctx.path, ctx.project.project_root.as_deref(), &pkg.script_entry_files)
        }) {
            return Vec::new();
        }

        let index = ctx.project.import_index();
        // `dead-export` is structurally cross-project — it needs to see
        // at least one OTHER file to count potential consumers. When
        // comply is invoked on a single file (pre-commit hook over a
        // staged-only diff, ad-hoc `comply src/shared/foo.ts`), the
        // index holds only the checked file and every export looks
        // dead. Skip in that mode; users have a workaround already in
        // place but the rule's premise can't be honoured.
        if index.indexed_paths().count() < 2 {
            return Vec::new();
        }
        let canon = std::fs::canonicalize(ctx.path).unwrap_or_else(|_| ctx.path.to_path_buf());

        // Framework entry points are consumed by framework tooling rather than
        // imported by application files in the index.
        if is_framework_entry_point(&canon, ctx.project) {
            return Vec::new();
        }
        let exports = index.get_exports(&canon);
        if exports.is_empty() {
            return Vec::new();
        }

        // If any importer uses namespace-import form, treat every export as
        // live — the index doesn't track which properties of `ns.*` are read.
        let reached_via_namespace = index
            .get_imports_to(&canon)
            .iter()
            .any(|imp| imp.kind == ImportKind::Namespace);
        if reached_via_namespace {
            return Vec::new();
        }

        let magic: std::collections::HashSet<&str> =
            ctx.project.framework_magic_exports().collect();

        // Types/interfaces consumed structurally by other exported functions
        // in the same file. Callers don't have to import the type name —
        // passing an object literal to the exported function is enough — so
        // the type's usage map looks empty but it is not dead.
        let structurally_consumed = collect_structurally_consumed_types(ctx.source, ctx.lang);

        // Names referenced anywhere in the file's body (outside their own
        // declaration site). Captures schema chains (`BaseSchema.extend(...)`,
        // `z.infer<typeof BaseSchema>`), object composition, and any other
        // intra-file re-use that doesn't go through the import index.
        let in_file_referenced = collect_in_file_referenced_names(ctx.source, ctx.lang);

        let mut diagnostics = Vec::new();
        for export in exports {
            if matches!(export.kind, ExportKind::StarReExport) {
                continue;
            }
            if magic.contains(export.name.as_str()) {
                continue;
            }
            if !index.get_usages(&canon, &export.name).is_empty() {
                continue;
            }
            if structurally_consumed.contains(export.name.as_str()) {
                continue;
            }
            if in_file_referenced.contains(export.name.as_str()) {
                continue;
            }
            diagnostics.push(Diagnostic {
                path: std::sync::Arc::clone(&ctx.path_arc),
                line: export.line,
                column: 1,
                rule_id: RULE_ID.into(),
                message: format!(
                    "export `{}` is never imported elsewhere in the project. \
                     Remove it or document why it's part of the public surface.",
                    export.name
                ),
                severity: Severity::Warning,
                span: None,
            });
        }
        diagnostics
    }
}

/// Collect the names of types/interfaces that appear inside an exported
/// function signature in the same file. Such names are consumed
/// structurally — callers reach them by passing an object literal to the
/// exported function, never by importing the type — so they look unused in
/// the import index even though they're load-bearing.
///
/// The walk only inspects nodes within `export_statement` whose declaration
/// is a function (`function_declaration`, `generator_function_declaration`).
/// Inside those, every `type_identifier` is collected. Type identifiers
/// that appear inside another exported `type_alias_declaration` or
/// `interface_declaration` are deliberately ignored — chaining one
/// "potentially dead" type through another doesn't make either of them live.
fn collect_structurally_consumed_types(source: &str, lang: crate::files::Language) -> HashSet<String> {
    let mut out = HashSet::new();
    let Some(grammar) = ts_language_for(lang) else {
        return out;
    };
    let mut parser = tree_sitter::Parser::new();
    if parser.set_language(&grammar).is_err() {
        return out;
    }
    let Some(tree) = parser.parse(source, None) else {
        return out;
    };
    let bytes = source.as_bytes();
    walk_tree(&tree, |node| {
        if node.kind() != "export_statement" {
            return;
        }
        for child in node.named_children(&mut node.walk()) {
            match child.kind() {
                "function_declaration" | "generator_function_declaration" => {
                    collect_type_identifiers(child, bytes, &mut out);
                }
                _ => {}
            }
        }
    });
    out
}

/// Collect names that occur 2+ times across the file's identifier and
/// type-identifier nodes at module top level (outside function bodies). The
/// declaration of an exported name contributes one occurrence; any additional
/// occurrence at top level means the name is consumed in-file by another
/// declaration (e.g. `BaseSchema.extend(...)`, `z.infer<typeof BaseSchema>`,
/// composition into another exported value).
///
/// Function bodies are excluded so that a type referenced only as a cast
/// inside an unrelated function (`{} as MyType`) does not silence the
/// diagnostic — see `still_flags_type_only_referenced_in_function_body`.
///
/// The heuristic deliberately ignores binding scope. A shadowed parameter
/// sharing a name with an export at top level would silence the diagnostic —
/// a false negative we accept in exchange for never re-flagging an export
/// that's genuinely re-used in the same file.
fn collect_in_file_referenced_names(source: &str, lang: crate::files::Language) -> HashSet<String> {
    let mut counts: std::collections::HashMap<String, u32> = std::collections::HashMap::new();
    let Some(grammar) = ts_language_for(lang) else {
        return HashSet::new();
    };
    let mut parser = tree_sitter::Parser::new();
    if parser.set_language(&grammar).is_err() {
        return HashSet::new();
    }
    let Some(tree) = parser.parse(source, None) else {
        return HashSet::new();
    };
    let bytes = source.as_bytes();
    let root = tree.root_node();
    let mut stack: Vec<tree_sitter::Node> = vec![root];
    while let Some(node) = stack.pop() {
        match node.kind() {
            "identifier" | "type_identifier" | "shorthand_property_identifier" => {
                if let Ok(text) = node.utf8_text(bytes) {
                    *counts.entry(text.to_string()).or_insert(0) += 1;
                }
            }
            _ => {}
        }
        for child in node.named_children(&mut node.walk()) {
            match child.kind() {
                // Skip function bodies — references inside them aren't a sign
                // the exported name is consumed by another module-level export.
                "statement_block" => continue,
                // Skip export clauses (`export { Foo as Bar }`) — the
                // identifiers there are re-export references, not in-file
                // consumers. Counting them would inflate `Foo`'s occurrence
                // count and silence dead-export when neither `Foo` nor `Bar`
                // is imported elsewhere.
                "export_clause" | "export_specifier" => continue,
                _ => {}
            }
            stack.push(child);
        }
    }
    counts
        .into_iter()
        .filter_map(|(name, n)| if n >= 2 { Some(name) } else { None })
        .collect()
}

/// Push the text of every `type_identifier` in `node`'s signature into `out`.
/// Only descends into `formal_parameters` and `return_type` children; skips
/// `statement_block` so that type casts or local variable annotations inside
/// the function body do not silence dead-export for types that appear nowhere
/// in the public signature.
fn collect_type_identifiers(node: tree_sitter::Node, source: &[u8], out: &mut HashSet<String>) {
    let mut stack = vec![node];
    while let Some(n) = stack.pop() {
        if n.kind() == "type_identifier" {
            if let Ok(text) = n.utf8_text(source) {
                out.insert(text.to_string());
            }
        }
        for child in n.named_children(&mut n.walk()) {
            if child.kind() == "statement_block" {
                continue;
            }
            stack.push(child);
        }
    }
}

/// True when `path` is listed as a CLI entry point in a `package.json`
/// `scripts` value (e.g. `"seed:dev": "bun run src/db/seed/dev.ts"`).
/// Compares the file's path relative to `project_root` (forward-slash,
/// no leading `./`) against the extracted `script_entry_files` list.
fn is_script_entry_point(
    path: &Path,
    project_root: Option<&Path>,
    script_entry_files: &[String],
) -> bool {
    if script_entry_files.is_empty() {
        return false;
    }
    let Some(root) = project_root else {
        return false;
    };
    let rel = path
        .strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/");
    script_entry_files.iter().any(|entry| *entry == rel)
}

/// Entry points we deliberately never flag: `main.*` and `index.*` directly
/// at the project root. Nested `index.ts` files (e.g. barrel files in
/// feature folders) are expected to be imported and stay subject to the rule.
fn is_entry_point(path: &Path, project_root: Option<&Path>) -> bool {
    let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
        return false;
    };
    if stem != "main" && stem != "index" {
        return false;
    }
    let Some(root) = project_root else {
        // No root detected (LSP / single-file) — err on the side of silence
        // for these conventional names.
        return true;
    };
    let Some(parent) = path.parent() else {
        return false;
    };
    let canon_parent = std::fs::canonicalize(parent).unwrap_or_else(|_| parent.to_path_buf());
    let canon_root = std::fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf());
    canon_parent == canon_root
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

    fn run_on_project(files: &[(&str, &str)], target_rel: &str) -> (TempDir, Vec<Diagnostic>) {
        run_on_project_with_pkg(None, files, target_rel)
    }

    fn run_on_project_with_pkg(
        package_json: Option<&str>,
        files: &[(&str, &str)],
        target_rel: &str,
    ) -> (TempDir, Vec<Diagnostic>) {
        let dir = TempDir::new().unwrap();
        if let Some(package_json) = package_json {
            fs::write(dir.path().join("package.json"), package_json).unwrap();
        }
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
        let file_ctx = FileCtx::build(&target_path, &source, Language::TypeScript, &project);
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
    fn skips_in_single_file_scan_mode() {
        // Regression for rbaumier/comply#33 — `comply src/shared/foo.ts`
        // sees only one indexed file, so it can't see consumers and
        // every export would falsely look dead. Skip in that mode.
        let files: Vec<(&str, &str)> = vec![
            ("foo.ts", "export function foo() {}"),
        ];
        let (_dir, diags) = run_on_project(&files, "foo.ts");
        assert!(diags.is_empty(), "single-file scan must not run dead-export");
    }

    #[test]
    fn flags_export_with_no_importer() {
        let files: Vec<(&str, &str)> = vec![
            ("tax.ts", "export function computeTax() {}"),
            ("other.ts", "export const y = 1;"),
        ];
        let (_dir, diags) = run_on_project(&files, "tax.ts");
        assert_eq!(diags.len(), 1, "computeTax is never imported");
        assert_eq!(diags[0].rule_id, "dead-export");
        assert!(
            diags[0].message.contains("computeTax"),
            "message should name the dead export, got: {}",
            diags[0].message
        );
    }

    #[test]
    fn allows_export_imported_elsewhere() {
        let files: Vec<(&str, &str)> = vec![
            ("tax.ts", "export function computeTax() {}"),
            ("app.ts", "import { computeTax } from './tax';"),
        ];
        let (_dir, diags) = run_on_project(&files, "tax.ts");
        assert!(diags.is_empty(), "computeTax is imported, no diagnostic");
    }

    #[test]
    fn ignores_root_entry_points() {
        // `index.ts` at the project root acts as the entry — not flagged.
        let files: Vec<(&str, &str)> = vec![
            ("index.ts", "export function bootstrap() {}"),
            ("other.ts", "export const z = 1;"),
        ];
        let (_dir, diags) = run_on_project(&files, "index.ts");
        assert!(diags.is_empty(), "root index.ts must not be flagged");
    }

    #[test]
    fn ignores_test_files() {
        let files: Vec<(&str, &str)> = vec![
            ("tax.test.ts", "export function fixture() {}"),
            ("app.ts", "export const z = 1;"),
        ];
        let (_dir, diags) = run_on_project(&files, "tax.test.ts");
        assert!(diags.is_empty(), "test files must not be flagged");
    }

    #[test]
    fn ignores_tanstack_router_lazy_file_imported_by_dash_prefixed_test() {
        // Regression for #78 — TanStack Router `.lazy.tsx` route exports a
        // component that's only consumed by a `-*.test.tsx` sibling. The
        // route file is a framework entry point, so dead-export must not
        // fire on its exports even if no other application file imports
        // them directly.
        let pkg = r#"{ "dependencies": { "@tanstack/react-router": "1.0.0" } }"#;
        let files: Vec<(&str, &str)> = vec![
            (
                "src/app/routes/_authed/index.lazy.tsx",
                "export function DashboardPage() { return null; }\n\
                 export const Route = createLazyFileRoute('/_authed/')({ component: DashboardPage });",
            ),
            (
                "src/app/routes/_authed/-index.test.tsx",
                "import { DashboardPage } from './index.lazy';\nDashboardPage;",
            ),
        ];
        let (_dir, diags) = run_on_project_with_pkg(
            Some(pkg),
            &files,
            "src/app/routes/_authed/index.lazy.tsx",
        );
        assert!(
            diags.is_empty(),
            ".lazy.tsx route is a framework entry; dead-export must not fire, got: {diags:?}"
        );
    }

    #[test]
    fn ignores_module_consumed_via_namespace_import() {
        // When `import * as ns from './m'` exists, individual symbol usages
        // are intentionally not linked; flagging every export would be noise.
        let files: Vec<(&str, &str)> = vec![
            ("m.ts", "export const a = 1; export const b = 2;"),
            ("app.ts", "import * as ns from './m';"),
        ];
        let (_dir, diags) = run_on_project(&files, "m.ts");
        assert!(
            diags.is_empty(),
            "namespace importer suppresses dead-export"
        );
    }

    #[test]
    fn flags_multiple_dead_exports_independently() {
        let files: Vec<(&str, &str)> = vec![
            ("m.ts", "export const a = 1;\nexport const b = 2;"),
            ("app.ts", "import { a } from './m';"),
        ];
        let (_dir, diags) = run_on_project(&files, "m.ts");
        assert_eq!(diags.len(), 1, "only `b` should be flagged");
        assert!(diags[0].message.contains('b'));
    }

    #[test]
    fn ignores_components_ui_directory() {
        // shadcn convention: drop-in components installed before any importer
        // exists must not be flagged.
        let files: Vec<(&str, &str)> = vec![
            ("components/ui/button.tsx", "export function Button() {}"),
            ("app.ts", "export const z = 1;"),
        ];
        let (_dir, diags) = run_on_project(&files, "components/ui/button.tsx");
        assert!(
            diags.is_empty(),
            "components/ui/* should be skipped: {diags:?}"
        );
    }

    #[test]
    fn ignores_src_components_ui_directory() {
        let files: Vec<(&str, &str)> = vec![
            ("src/components/ui/card.tsx", "export function Card() {}"),
            ("src/app.ts", "export const z = 1;"),
        ];
        let (_dir, diags) = run_on_project(&files, "src/components/ui/card.tsx");
        assert!(
            diags.is_empty(),
            "src/components/ui/* should be skipped: {diags:?}"
        );
    }

    #[test]
    fn ignores_lib_ui_directory() {
        let files: Vec<(&str, &str)> = vec![
            ("lib/ui/avatar.tsx", "export function Avatar() {}"),
            ("app.ts", "export const z = 1;"),
        ];
        let (_dir, diags) = run_on_project(&files, "lib/ui/avatar.tsx");
        assert!(diags.is_empty(), "lib/ui/* should be skipped: {diags:?}");
    }

    #[test]
    fn ignores_generated_files() {
        let files: Vec<(&str, &str)> = vec![
            (
                "schema.ts",
                "// @generated by codegen. do not edit.\nexport const TableA = {};",
            ),
            ("app.ts", "export const z = 1;"),
        ];
        let (_dir, diags) = run_on_project(&files, "schema.ts");
        assert!(
            diags.is_empty(),
            "@generated files should be skipped: {diags:?}"
        );
    }

    #[test]
    fn ignores_block_comment_generated_marker() {
        let files: Vec<(&str, &str)> = vec![
            ("schema.ts", "/* @generated */\nexport const Settings = {};"),
            ("app.ts", "export const z = 1;"),
        ];
        let (_dir, diags) = run_on_project(&files, "schema.ts");
        assert!(
            diags.is_empty(),
            "/* @generated */ should be skipped: {diags:?}"
        );
    }

    #[test]
    fn no_crash_on_multibyte_generated_scan() {
        let files: Vec<(&str, &str)> = vec![
            ("tax.ts", "// مثال عربي\nexport function computeTax() {}"),
            ("app.ts", "export const z = 1;"),
        ];
        let (_dir, diags) = run_on_project(&files, "tax.ts");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn still_flags_components_outside_ui_dir() {
        let files: Vec<(&str, &str)> = vec![
            (
                "components/feature/header.tsx",
                "export function Header() {}",
            ),
            ("app.ts", "export const z = 1;"),
        ];
        let (_dir, diags) = run_on_project(&files, "components/feature/header.tsx");
        assert_eq!(
            diags.len(),
            1,
            "components/<feature>/ should still be flagged"
        );
    }

    #[test]
    fn skips_type_used_in_exported_function_signature() {
        // Regression for #100 — `FormServerErrorTarget` parameterizes
        // `applyProblemErrorToForm`'s second argument. Callers pass an
        // object literal into the function and never import the type by
        // name, so the import index sees zero usages. The type IS still
        // consumed structurally; dead-export must keep quiet.
        let files: Vec<(&str, &str)> = vec![
            (
                "form-server-errors.ts",
                "export type FormServerErrorTarget = { field: string };\n\
                 export function applyProblemErrorToForm(error: Error, target: FormServerErrorTarget): void {}\n",
            ),
            (
                "app.ts",
                "import { applyProblemErrorToForm } from './form-server-errors';\n\
                 applyProblemErrorToForm(new Error('x'), { field: 'email' });\n",
            ),
        ];
        let (_dir, diags) = run_on_project(&files, "form-server-errors.ts");
        assert!(
            diags.iter().all(|d| !d.message.contains("FormServerErrorTarget")),
            "type used structurally by an exported function must not be flagged, got: {diags:?}"
        );
    }

    #[test]
    fn still_flags_type_not_referenced_by_any_export() {
        // Sibling guard for #100 — a truly orphan type with no importer
        // and no in-file consumer must still be flagged.
        let files: Vec<(&str, &str)> = vec![
            ("types.ts", "export type Orphan = { a: number };\n"),
            ("other.ts", "export const z = 1;\n"),
        ];
        let (_dir, diags) = run_on_project(&files, "types.ts");
        assert_eq!(diags.len(), 1, "orphan type should still be flagged");
        assert!(diags[0].message.contains("Orphan"));
    }

    #[test]
    fn still_flags_type_only_referenced_in_function_body() {
        // Regression — a type that appears only as a cast (`as MyType`) inside
        // a function body, not in the function's signature, must still be
        // flagged as dead. Previously `collect_type_identifiers` walked all
        // descendants including `statement_block`, which caused the body cast
        // to silently suppress the diagnostic.
        let files: Vec<(&str, &str)> = vec![
            (
                "casts.ts",
                "export type BodyOnly = { x: number };\n\
                 export function doStuff() {\n\
                   const v = {} as BodyOnly;\n\
                   return v;\n\
                 }\n",
            ),
            ("other.ts", "import { doStuff } from './casts';\ndoStuff();\n"),
        ];
        let (_dir, diags) = run_on_project(&files, "casts.ts");
        assert!(
            diags.iter().any(|d| d.message.contains("BodyOnly")),
            "type only cast inside body should still be flagged, got: {diags:?}"
        );
    }

    #[test]
    fn skips_schema_reused_via_extend_in_same_file() {
        // Regression for #95 — `TeamCentralCodeSchema` is consumed in-file by
        // `TeamCentralCodeSchema.extend(...)` and `z.infer<typeof TeamCentralCodeSchema>`.
        // Only the derived schema is imported elsewhere; dead-export must not
        // flag the base.
        let files: Vec<(&str, &str)> = vec![
            (
                "schemas.ts",
                "import { z } from 'zod';\n\
                 export const TeamCentralCodeSchema = z.object({ code: z.string() });\n\
                 export type TeamCentralCode = z.infer<typeof TeamCentralCodeSchema>;\n\
                 export const TeamCentralCodeWithCentraleResponseSchema = TeamCentralCodeSchema.extend({ extra: z.string() });\n\
                 export type TeamCentralCodeWithCentraleResponse = z.infer<typeof TeamCentralCodeWithCentraleResponseSchema>;\n",
            ),
            (
                "app.ts",
                "import { TeamCentralCodeWithCentraleResponseSchema } from './schemas';\n\
                 TeamCentralCodeWithCentraleResponseSchema.parse({});\n",
            ),
        ];
        let (_dir, diags) = run_on_project(&files, "schemas.ts");
        assert!(
            diags.iter().all(|d| !d.message.contains("TeamCentralCodeSchema")),
            "base schema reused in-file via .extend / z.infer<typeof> must not be flagged, got: {diags:?}"
        );
        assert!(
            diags.iter().all(|d| !d.message.contains("TeamCentralCode\"")),
            "base type reused in-file must not be flagged, got: {diags:?}"
        );
    }

    #[test]
    fn still_flags_export_only_re_exported_via_alias() {
        // Regression — `export { Foo as Bar }` used to inflate `Foo`'s
        // in-file reference count to 2, silencing dead-export even when
        // neither `Foo` nor `Bar` is imported by any other file.
        let files: Vec<(&str, &str)> = vec![
            (
                "reexport.ts",
                "export const Foo = 1;\nexport { Foo as Bar };\n",
            ),
            ("other.ts", "export const z = 1;\n"),
        ];
        let (_dir, diags) = run_on_project(&files, "reexport.ts");
        let names: Vec<&str> = diags.iter().map(|d| d.message.as_str()).collect();
        assert!(
            diags.iter().any(|d| d.message.contains("`Foo`")),
            "Foo is never imported — should be flagged, got: {names:?}"
        );
        assert!(
            diags.iter().any(|d| d.message.contains("`Bar`")),
            "Bar is never imported — should be flagged, got: {names:?}"
        );
    }

    #[test]
    fn ignores_tanstack_router_non_lazy_route_file_with_dollar_params() {
        // Regression for #382 — `users.$userId.tsx` in a `/routes/` directory
        // is a TanStack Router file-based route. Its `Route` export is a magic
        // export consumed by the router tree, not imported by application
        // files. dead-export must not fire.
        let pkg = r#"{ "dependencies": { "@tanstack/react-router": "1.0.0" } }"#;
        let files: Vec<(&str, &str)> = vec![
            (
                "src/app/routes/users.$userId.tsx",
                "export const Route = createLazyFileRoute('/users/$userId')({});",
            ),
            (
                "src/generated/routeTree.ts",
                "// @generated by @tanstack/router-cli\nexport const routeTree = {};",
            ),
            (
                "src/app/routes/-users.$userId.test.tsx",
                "import { UsersUserIdRoute } from '../../generated/routeTree';\nconst r = UsersUserIdRoute;",
            ),
        ];
        let (_dir, diags) = run_on_project_with_pkg(
            Some(pkg),
            &files,
            "src/app/routes/users.$userId.tsx",
        );
        assert!(
            diags.is_empty(),
            "route file in /routes/ is a framework entry — dead-export must not fire: {diags:?}"
        );
    }

    #[test]
    fn ignores_tanstack_start_router_factory_export_issue_495() {
        // Regression for #495 — TanStack Start's `getRouter`/`createRouter`
        // factory in `router.tsx` is consumed only by the gitignored
        // `routeTree.gen.ts` (via `import type { getRouter }` and the
        // `Register` interface). That file is absent from the index, so the
        // export looks dead. It's a framework magic export — never flag it.
        let pkg = r#"{ "dependencies": { "@tanstack/react-start": "1.0.0" } }"#;
        let files: Vec<(&str, &str)> = vec![
            (
                "src/app/router.tsx",
                "export const getRouter = (() => (): Router => buildRouter())();",
            ),
            ("src/app.ts", "export const z = 1;"),
        ];
        let (_dir, diags) = run_on_project_with_pkg(Some(pkg), &files, "src/app/router.tsx");
        assert!(
            diags.iter().all(|d| !d.message.contains("getRouter")),
            "TanStack Start router factory must not be flagged, got: {diags:?}"
        );
    }

    #[test]
    fn ignores_framework_entry_file_names() {
        let files: Vec<(&str, &str)> = vec![
            ("src/routeTree.gen.ts", "export const routeTree = {};"),
            ("src/app.ts", "export const z = 1;"),
        ];
        let pkg = r#"{"dependencies":{"@tanstack/react-router":"1"}}"#;
        let (_dir, diags) = run_on_project_with_pkg(Some(pkg), &files, "src/routeTree.gen.ts");
        assert!(
            diags.is_empty(),
            "generated TanStack route tree should be a framework entry point: {diags:?}"
        );
    }

    // Regression tests for issue #446

    #[test]
    fn no_fp_for_export_consumed_by_test_file() {
        // Regression for #446 — `renderWithProviders` is exported from a
        // test-helpers file that is NOT itself a test file (no `.test.` in name,
        // not in a `__tests__/` dir). It is imported by test files; dead-export
        // must not fire because test files ARE part of the import graph.
        let files: Vec<(&str, &str)> = vec![
            (
                "src/app/test-helpers/index.ts",
                "export function renderWithProviders() {}",
            ),
            (
                "src/features/user/user.test.ts",
                "import { renderWithProviders } from '../../app/test-helpers';\nrenderWithProviders();",
            ),
            ("src/app.ts", "export const z = 1;"),
        ];
        let (_dir, diags) = run_on_project(&files, "src/app/test-helpers/index.ts");
        assert!(
            diags.iter().all(|d| !d.message.contains("renderWithProviders")),
            "test-helper export consumed by test file must not be flagged, got: {diags:?}"
        );
    }

    #[test]
    fn no_fp_for_package_json_script_entry_point() {
        // Regression for #446 — `seedDevData` is exported from a file that is
        // invoked as a CLI entry point via a package.json script
        // (`"seed:dev": "bun run src/db/seed/dev.ts"`). No TS file imports it.
        // The file path matches the script entry point pattern, so dead-export
        // must not fire.
        let pkg = r#"{
            "scripts": {
                "seed:dev": "bun run src/db/seed/dev.ts",
                "delete-user": "bun run src/scripts/deleteUser.ts"
            }
        }"#;
        let files: Vec<(&str, &str)> = vec![
            (
                "src/db/seed/dev.ts",
                "export async function seedDevData(): Promise<void> {}",
            ),
            ("src/app.ts", "export const z = 1;"),
        ];
        let (_dir, diags) = run_on_project_with_pkg(Some(pkg), &files, "src/db/seed/dev.ts");
        assert!(
            diags.iter().all(|d| !d.message.contains("seedDevData")),
            "CLI entry point export must not be flagged, got: {diags:?}"
        );
    }

    #[test]
    fn no_fp_for_second_package_json_script_entry_point() {
        // Regression for #446 — another script entry point
        let pkg = r#"{
            "scripts": {
                "delete-user": "bun run src/scripts/deleteUser.ts"
            }
        }"#;
        let files: Vec<(&str, &str)> = vec![
            (
                "src/scripts/deleteUser.ts",
                "export async function deleteUser(id: string): Promise<void> {}",
            ),
            ("src/app.ts", "export const z = 1;"),
        ];
        let (_dir, diags) =
            run_on_project_with_pkg(Some(pkg), &files, "src/scripts/deleteUser.ts");
        assert!(
            diags.iter().all(|d| !d.message.contains("deleteUser")),
            "CLI entry point export must not be flagged, got: {diags:?}"
        );
    }
}
