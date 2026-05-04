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
use oxc_span::GetSpan;

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
            if let ExportDefaultDeclarationKind::FunctionDeclaration(f) = &export.declaration {
                if let Some(ref id) = f.id {
                    let (line, _) = byte_offset_to_line_col(source, f.span.start as usize);
                    out.entry(id.name.to_string()).or_insert(DeclInfo {
                        line,
                        exported: true,
                    });
                }
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
    if let Declaration::FunctionDeclaration(f) = decl {
        if let Some(ref id) = f.id {
            let (line, _) = byte_offset_to_line_col(source, f.span.start as usize);
            out.entry(id.name.to_string()).or_insert(DeclInfo {
                line,
                exported,
            });
        }
    }
}
