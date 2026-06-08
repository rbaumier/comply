//! inconsistent-function-call oxc backend.
//!
//! Collects every `function_declaration` in the file, then scans all call
//! sites for each name. If a function is called both as `new Foo(...)` and
//! `Foo(...)`, emit one diagnostic per inconsistent call site.

use std::collections::HashMap;

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::project::import_index::CallKind;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::*;

pub struct Check;

/// Where a function was declared and whether it is exported.
#[derive(Debug, Clone)]
struct DeclInfo {
    line: usize,
    exported: bool,
}

/// A call or `new` site.
#[derive(Debug, Clone)]
struct Site {
    path: std::path::PathBuf,
    line: usize,
    column: usize,
    byte_offset: usize,
    byte_len: usize,
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[]
    }

    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        let program = semantic.nodes().program();

        // 1. Collect every function_declaration name + whether it is exported.
        let mut declared: HashMap<String, DeclInfo> = HashMap::new();
        collect_function_declarations(program, ctx.source, &mut declared);
        if declared.is_empty() {
            return Vec::new();
        }

        // 2. Scan every call site in THIS file.
        let declared_names: Vec<String> = declared.keys().cloned().collect();
        let mut new_sites: HashMap<String, Vec<Site>> = HashMap::new();
        let mut plain_sites: HashMap<String, Vec<Site>> = HashMap::new();

        for node in semantic.nodes().iter() {
            match node.kind() {
                AstKind::NewExpression(new_expr) => {
                    if let Expression::Identifier(callee) = &new_expr.callee {
                        let name = callee.name.as_str();
                        if declared_names.iter().any(|n| n == name) {
                            let span = new_expr.span;
                            let (line, column) =
                                byte_offset_to_line_col(ctx.source, span.start as usize);
                            new_sites
                                .entry(name.to_string())
                                .or_default()
                                .push(Site {
                                    path: ctx.path.to_path_buf(),
                                    line,
                                    column,
                                    byte_offset: span.start as usize,
                                    byte_len: (span.end - span.start) as usize,
                                });
                        }
                    }
                }
                AstKind::CallExpression(call_expr) => {
                    if let Expression::Identifier(callee) = &call_expr.callee {
                        let name = callee.name.as_str();
                        if declared_names.iter().any(|n| n == name) {
                            let span = call_expr.span;
                            let (line, column) =
                                byte_offset_to_line_col(ctx.source, span.start as usize);
                            plain_sites
                                .entry(name.to_string())
                                .or_default()
                                .push(Site {
                                    path: ctx.path.to_path_buf(),
                                    line,
                                    column,
                                    byte_offset: span.start as usize,
                                    byte_len: (span.end - span.start) as usize,
                                });
                        }
                    }
                }
                _ => {}
            }
        }

        // 3. Merge in cross-file call sites for exported functions.
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
        //    every call site.
        let mut diagnostics = Vec::new();
        for (name, info) in &declared {
            let news = new_sites.get(name);
            let plains = plain_sites.get(name);
            let (Some(news), Some(plains)) = (news, plains) else {
                continue;
            };
            if news.is_empty() || plains.is_empty() {
                continue;
            }

            let decl_line = info.line;
            let decl_path = ctx.path.display().to_string();
            for site in news.iter().chain(plains.iter()) {
                diagnostics.push(Diagnostic {
                    path: site.path.clone().into(),
                    line: site.line,
                    column: site.column,
                    rule_id: super::META.id.into(),
                    message: format!(
                        "Function `{name}` (declared in {decl_path}:{decl_line}) is called both with and without `new`. Pick one style — use `new` for constructors, never for plain functions."
                    ),
                    severity: Severity::Error,
                    span: Some((site.byte_offset, site.byte_len)),
                });
            }
        }

        diagnostics
    }
}

/// Walk the program body and record every `function_declaration` name.
/// A declaration is marked `exported` when it appears inside an export.
/// Nested declarations count too (walked recursively via statements).
fn collect_function_declarations(
    program: &Program<'_>,
    source: &str,
    out: &mut HashMap<String, DeclInfo>,
) {
    for stmt in &program.body {
        collect_from_statement(stmt, source, false, out);
    }
}

fn collect_from_statement(
    stmt: &Statement<'_>,
    source: &str,
    exported: bool,
    out: &mut HashMap<String, DeclInfo>,
) {
    match stmt {
        Statement::FunctionDeclaration(f) => {
            if let Some(ref id) = f.id {
                let (line, _) = byte_offset_to_line_col(source, f.span.start as usize);
                out.entry(id.name.to_string()).or_insert(DeclInfo {
                    line,
                    exported,
                });
            }
        }
        Statement::ExportNamedDeclaration(export) => {
            if let Some(ref decl) = export.declaration {
                collect_from_declaration(decl, source, true, out);
            }
        }
        Statement::ExportDefaultDeclaration(export) => {
            if let ExportDefaultDeclarationKind::FunctionDeclaration(f) = &export.declaration
                && let Some(ref id) = f.id {
                    let (line, _) = byte_offset_to_line_col(source, f.span.start as usize);
                    out.entry(id.name.to_string()).or_insert(DeclInfo {
                        line,
                        exported: true,
                    });
                }
        }
        Statement::BlockStatement(block) => {
            for s in &block.body {
                collect_from_statement(s, source, false, out);
            }
        }
        _ => {}
    }
}

fn collect_from_declaration(
    decl: &Declaration<'_>,
    source: &str,
    exported: bool,
    out: &mut HashMap<String, DeclInfo>,
) {
    if let Declaration::FunctionDeclaration(f) = decl
        && let Some(ref id) = f.id {
            let (line, _) = byte_offset_to_line_col(source, f.span.start as usize);
            out.entry(id.name.to_string()).or_insert(DeclInfo {
                line,
                exported,
            });
        }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::files::{Language, SourceFile};
    use crate::project::ProjectCtx;
    use std::fs;
    use std::path::PathBuf;
    use tempfile::TempDir;



    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
    }

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
        crate::rules::test_helpers::run_oxc_ts_with_project(&source, &Check, project)
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
}
