//! inconsistent-function-call backend.
//!
//! Collects every `function_declaration` in the file, then scans all call
//! sites for each name. If a function is called both as `new Foo(...)` and
//! `Foo(...)`, emit one diagnostic per inconsistent call site.
//!
//! When the function is also `export`ed, the check extends to cross-file
//! call sites via `ProjectCtx::import_index()` — a `Widget` that is called
//! with `new` in `a.ts` but without `new` in `b.ts` is just as inconsistent
//! as mixing styles in the same file.
//!
//! Classes are excluded — they are always called with `new` (the grammar
//! uses `class_declaration`, not `function_declaration`). Arrow functions
//! and `const foo = function() {}` are also excluded: arrows cannot be
//! constructed at all (the engine throws on `new`), and named function
//! expressions are rare enough that the extra noise isn't worth it.

use std::collections::HashMap;

use crate::diagnostic::{Diagnostic, Severity};
use crate::project::import_index::CallKind;

crate::ast_check! { on ["program"] => |node, source, ctx, diagnostics|
    // Run once per file, at the root.
    // 1. Collect every top-level `function_declaration` name + whether it is
    //    exported (so cross-file call-sites can join the analysis).
    let mut declared: HashMap<String, DeclInfo> = HashMap::new();
    collect_function_declarations(node, source, &mut declared);
    if declared.is_empty() {
        return;
    }

    // 2. Scan every call site in THIS file.
    let declared_names: HashMap<String, tree_sitter::Range> =
        declared.iter().map(|(n, d)| (n.clone(), d.range)).collect();
    let mut new_sites: HashMap<String, Vec<Site>> = HashMap::new();
    let mut plain_sites: HashMap<String, Vec<Site>> = HashMap::new();
    collect_calls(node, source, &declared_names, ctx.path, &mut new_sites, &mut plain_sites);

    // 3. Merge in cross-file call sites for exported functions. The index
    //    keys by *exported* name, so a renamed import (`import { Widget as W }`)
    //    is transparently folded back into the `Widget` bucket.
    let index = ctx.project.import_index();
    for (name, info) in &declared {
        if !info.exported {
            continue;
        }
        for site in index.get_call_sites(ctx.path, name) {
            let bucket = match site.kind {
                CallKind::New => new_sites.entry(name.clone()).or_default(),
                CallKind::Call => plain_sites.entry(name.clone()).or_default(),
            };
            bucket.push(Site {
                path: site.path.clone(),
                line: site.line,
                column: site.column,
                byte_offset: site.byte_offset,
                byte_len: site.byte_len,
            });
        }
    }

    // 4. For every function called in BOTH styles, emit a diagnostic on
    //    every call site so the user sees every inconsistent location.
    for (name, info) in &declared {
        let news = new_sites.get(name);
        let plains = plain_sites.get(name);
        let (Some(news), Some(plains)) = (news, plains) else { continue };
        if news.is_empty() || plains.is_empty() {
            continue;
        }

        let decl_line = info.range.start_point.row + 1;
        let decl_path = ctx.path.display().to_string();
        for site in news.iter().chain(plains.iter()) {
            diagnostics.push(Diagnostic {
                path: site.path.clone().into(),
                line: site.line,
                column: site.column,
                rule_id: "inconsistent-function-call".into(),
                message: format!(
                    "Function `{name}` (declared in {decl_path}:{decl_line}) is called both with and without `new`. Pick one style — use `new` for constructors, never for plain functions."
                ),
                severity: Severity::Error,
                span: Some((site.byte_offset, site.byte_len)),
            });
        }
    }
}

/// Where a function was declared and whether it is exported. Exported
/// functions are cross-file candidates; purely-local functions are not.
#[derive(Debug, Clone, Copy)]
struct DeclInfo {
    range: tree_sitter::Range,
    exported: bool,
}

/// A call or `new` site, abstracted over in-file and cross-file origins so
/// the diagnostic emitter treats both uniformly.
#[derive(Debug, Clone)]
struct Site {
    path: std::path::PathBuf,
    line: usize,
    column: usize,
    byte_offset: usize,
    byte_len: usize,
}

/// Walk the tree and record every `function_declaration` name with its
/// source range. A declaration is marked `exported` when its parent node is
/// `export_statement` (handles both `export function Foo()` and
/// `export default function Foo()`). Nested declarations count too.
fn collect_function_declarations(
    root: tree_sitter::Node<'_>,
    source: &[u8],
    out: &mut HashMap<String, DeclInfo>,
) {
    let mut stack = vec![root];
    while let Some(node) = stack.pop() {
        if node.kind() == "function_declaration"
            && let Some(name_node) = node.child_by_field_name("name")
            && let Ok(name) = name_node.utf8_text(source)
        {
            let exported = node
                .parent()
                .is_some_and(|p| p.kind() == "export_statement");
            out.entry(name.to_string()).or_insert(DeclInfo {
                range: node.range(),
                exported,
            });
        }
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            stack.push(child);
        }
    }
}

/// Walk the tree and bucket every in-file call site by name:
/// * `new Foo(...)` → `new_sites["Foo"]`
/// * `Foo(...)`     → `plain_sites["Foo"]`
///
/// Only names declared via `function_declaration` are tracked (the caller
/// passes in the set). Sites carry the file path so they merge uniformly
/// with cross-file sites pulled from the `ImportIndex`.
fn collect_calls(
    root: tree_sitter::Node<'_>,
    source: &[u8],
    declared: &HashMap<String, tree_sitter::Range>,
    path: &std::path::Path,
    new_sites: &mut HashMap<String, Vec<Site>>,
    plain_sites: &mut HashMap<String, Vec<Site>>,
) {
    let mut stack = vec![root];
    while let Some(node) = stack.pop() {
        match node.kind() {
            "new_expression" => {
                if let Some(callee) = node.child_by_field_name("constructor")
                    && callee.kind() == "identifier"
                    && let Ok(name) = callee.utf8_text(source)
                    && declared.contains_key(name)
                {
                    new_sites
                        .entry(name.to_string())
                        .or_default()
                        .push(site_from_node(path, node));
                }
            }
            "call_expression" => {
                if let Some(callee) = node.child_by_field_name("function")
                    && callee.kind() == "identifier"
                    && let Ok(name) = callee.utf8_text(source)
                    && declared.contains_key(name)
                {
                    plain_sites
                        .entry(name.to_string())
                        .or_default()
                        .push(site_from_node(path, node));
                }
            }
            _ => {}
        }
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            stack.push(child);
        }
    }
}

fn site_from_node(path: &std::path::Path, node: tree_sitter::Node<'_>) -> Site {
    let range = node.range();
    Site {
        path: path.to_path_buf(),
        line: range.start_point.row + 1,
        column: range.start_point.column + 1,
        byte_offset: range.start_byte,
        byte_len: range.end_byte - range.start_byte,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_mixed_new_and_plain_call() {
        let src = r#"
function Widget() { this.id = 1; }
const a = new Widget();
const b = Widget();
"#;
        let d = run_on(src);
        assert_eq!(d.len(), 2);
        assert!(d.iter().all(|x| x.message.contains("Widget")));
    }

    #[test]
    fn allows_only_new_calls() {
        let src = r#"
function Widget() { this.id = 1; }
const a = new Widget();
const b = new Widget();
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_only_plain_calls() {
        let src = r#"
function helper(x) { return x + 1; }
const a = helper(1);
const b = helper(2);
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_classes() {
        // Classes are always called with `new`; even if someone tries
        // `MyClass()` the grammar treats the class body separately.
        let src = r#"
class MyClass { constructor() {} }
const a = new MyClass();
const b = new MyClass();
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_arrow_functions() {
        let src = r#"
const toId = (x) => x.id;
const a = toId({ id: 1 });
const b = toId({ id: 2 });
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn handles_multiple_functions_independently() {
        let src = r#"
function Widget() { this.id = 1; }
function helper(x) { return x; }
const a = new Widget();
const b = new Widget();
const c = helper(1);
const d = helper(2);
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_only_the_mixed_one() {
        let src = r#"
function Widget() { this.id = 1; }
function helper(x) { return x; }
const a = new Widget();
const b = Widget();
const c = helper(1);
"#;
        let d = run_on(src);
        assert_eq!(d.len(), 2);
        assert!(d.iter().all(|x| x.message.contains("Widget")));
    }

    #[test]
    fn flags_three_way_imbalance() {
        // Two `new`, one plain — all three sites are inconsistent, so we
        // expect three diagnostics.
        let src = r#"
function Widget() { this.id = 1; }
const a = new Widget();
const b = new Widget();
const c = Widget();
"#;
        let d = run_on(src);
        assert_eq!(d.len(), 3);
    }

    // -------- cross-file tests (ImportIndex-backed) --------

    use crate::files::{Language, SourceFile};
    use crate::project::ProjectCtx;
    use std::fs;
    use std::path::PathBuf;
    use tempfile::TempDir;

    /// Build a tiny multi-file project on disk, return the tempdir, a
    /// `ProjectCtx` with a populated `ImportIndex`, and the canonicalized
    /// paths of each written file in input order.
    fn build_project(files: &[(&str, &str)]) -> (TempDir, ProjectCtx, Vec<PathBuf>) {
        let dir = TempDir::new().unwrap();
        let mut sources = Vec::new();
        let mut paths = Vec::new();
        for (rel, content) in files {
            let p = dir.path().join(rel);
            if let Some(parent) = p.parent() {
                fs::create_dir_all(parent).unwrap();
            }
            fs::write(&p, content).unwrap();
            let lang = Language::from_path(&p).unwrap();
            sources.push(SourceFile {
                path: p.clone(),
                language: lang,
            });
            paths.push(fs::canonicalize(&p).unwrap());
        }
        let refs: Vec<&SourceFile> = sources.iter().collect();
        let project = ProjectCtx::for_test_with_files(&refs);
        (dir, project, paths)
    }

    fn run_on_file(project: &ProjectCtx, path: &std::path::Path) -> Vec<Diagnostic> {
        let source = fs::read_to_string(path).unwrap();
        crate::rules::test_helpers::run_ts_with_project_and_path(&source, &Check, project, path)
    }

    #[test]
    fn cross_file_flags_inconsistent_new_and_call() {
        let (_dir, project, paths) = build_project(&[
            ("utils.ts", "export function Widget() { this.id = 1; }\n"),
            (
                "file-a.ts",
                "import { Widget } from './utils';\nconst a = new Widget();\n",
            ),
            (
                "file-b.ts",
                "import { Widget } from './utils';\nconst b = Widget();\n",
            ),
        ]);
        let diags = run_on_file(&project, &paths[0]);
        // One `new` site in file-a.ts + one plain site in file-b.ts.
        assert_eq!(diags.len(), 2, "got: {diags:#?}");
        assert!(diags.iter().all(|d| d.message.contains("Widget")));
        // Diagnostics are emitted on the call-sites in the importer files —
        // not on the declaring file.
        let mut importer_paths: Vec<std::path::PathBuf> =
            diags.iter().map(|d| d.path.to_path_buf()).collect();
        importer_paths.sort();
        let mut expected = vec![paths[1].clone(), paths[2].clone()];
        expected.sort();
        assert_eq!(importer_paths, expected);
    }

    #[test]
    fn cross_file_allows_consistent_new() {
        let (_dir, project, paths) = build_project(&[
            ("utils.ts", "export function Widget() { this.id = 1; }\n"),
            (
                "file-a.ts",
                "import { Widget } from './utils';\nconst a = new Widget();\n",
            ),
            (
                "file-b.ts",
                "import { Widget } from './utils';\nconst b = new Widget();\n",
            ),
        ]);
        assert!(run_on_file(&project, &paths[0]).is_empty());
    }

    #[test]
    fn cross_file_allows_consistent_plain_calls() {
        let (_dir, project, paths) = build_project(&[
            ("utils.ts", "export function helper(x) { return x + 1; }\n"),
            (
                "file-a.ts",
                "import { helper } from './utils';\nhelper(1);\n",
            ),
            (
                "file-b.ts",
                "import { helper } from './utils';\nhelper(2);\n",
            ),
        ]);
        assert!(run_on_file(&project, &paths[0]).is_empty());
    }

    #[test]
    fn cross_file_ignores_classes() {
        // `class_declaration` is never collected, so an imported class
        // called with `new` in one file and (hypothetically) without in
        // another is out of scope for this rule.
        let (_dir, project, paths) = build_project(&[
            ("m.ts", "export class MyClass { constructor() {} }\n"),
            (
                "a.ts",
                "import { MyClass } from './m';\nconst a = new MyClass();\n",
            ),
            (
                "b.ts",
                "import { MyClass } from './m';\nconst b = new MyClass();\n",
            ),
        ]);
        assert!(run_on_file(&project, &paths[0]).is_empty());
    }

    #[test]
    fn cross_file_merges_with_in_file_calls() {
        // Declaration + one in-file `new Widget()`, plus a cross-file
        // plain call — the mix is still inconsistent.
        let (_dir, project, paths) = build_project(&[
            (
                "utils.ts",
                "export function Widget() { this.id = 1; }\nconst local = new Widget();\n",
            ),
            (
                "app.ts",
                "import { Widget } from './utils';\nconst a = Widget();\n",
            ),
        ]);
        let diags = run_on_file(&project, &paths[0]);
        assert_eq!(diags.len(), 2, "got: {diags:#?}");
    }

    #[test]
    fn cross_file_translates_renamed_import() {
        // `import { Widget as W }` — the index keys by the exported name,
        // so `new W()` is folded back into Widget's bucket.
        let (_dir, project, paths) = build_project(&[
            ("utils.ts", "export function Widget() { this.id = 1; }\n"),
            (
                "app.ts",
                "import { Widget as W } from './utils';\nconst a = new W();\nconst b = W();\n",
            ),
        ]);
        let diags = run_on_file(&project, &paths[0]);
        assert_eq!(diags.len(), 2, "got: {diags:#?}");
    }

    #[test]
    fn non_exported_function_skips_cross_file_lookup() {
        // `Widget` is not exported. Cross-file usage is impossible; in-file
        // consistency (only `new`) means no diagnostic.
        let (_dir, project, paths) = build_project(&[(
            "utils.ts",
            "function Widget() { this.id = 1; }\nconst a = new Widget();\n",
        )]);
        assert!(run_on_file(&project, &paths[0]).is_empty());
    }
}
