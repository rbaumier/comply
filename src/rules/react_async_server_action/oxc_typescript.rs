//! react-async-server-action OxcCheck backend — server actions must be async.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{CheckCtx, OxcCheck};
use oxc_semantic::Semantic;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["use server"])
    }

    fn run_on_semantic<'a>(
        &self,
        semantic: &'a Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        use oxc_ast::AstKind;
        let mut diagnostics = Vec::new();
        let Some(prog) = semantic.nodes().iter().find_map(|n| {
            if let AstKind::Program(p) = n.kind() { Some(p) } else { None }
        }) else {
            return diagnostics;
        };

        // Check for file-level "use server" directive.
        let file_level_use_server = prog.directives.iter().any(|d| d.expression.value == "use server");

        if file_level_use_server {
            // All exported function declarations must be async.
            for stmt in &prog.body {
                let oxc_ast::ast::Statement::ExportNamedDeclaration(export) = stmt else {
                    continue;
                };
                let Some(ref decl) = export.declaration else {
                    continue;
                };
                if let oxc_ast::ast::Declaration::FunctionDeclaration(func) = decl {
                    if !func.r#async {
                        let (line, column) =
                            byte_offset_to_line_col(ctx.source, func.span.start as usize);
                        diagnostics.push(Diagnostic {
                            path: Arc::clone(&ctx.path_arc),
                            line,
                            column,
                            rule_id: super::META.id.into(),
                            message: "Server action must be `async`. This file has \
                                      `\"use server\"` at the top \u{2014} all exported \
                                      functions must be async."
                                .into(),
                            severity: Severity::Error,
                            span: None,
                        });
                    }
                }
            }
        }

        // Check for inline "use server" inside function bodies.
        for node in semantic.nodes().iter() {
            let body_stmts = match node.kind() {
                AstKind::Function(func) => {
                    if func.r#async {
                        continue;
                    }
                    func.body.as_ref().map(|b| &b.statements)
                }
                AstKind::ArrowFunctionExpression(arrow) => {
                    if arrow.r#async {
                        continue;
                    }
                    Some(&arrow.body.statements)
                }
                _ => continue,
            };
            let Some(stmts) = body_stmts else { continue };
            let has_use_server = stmts.iter().any(|stmt| {
                if let oxc_ast::ast::Statement::ExpressionStatement(expr) = stmt {
                    if let oxc_ast::ast::Expression::StringLiteral(lit) = &expr.expression {
                        return lit.value == "use server";
                    }
                }
                false
            });
            if has_use_server {
                let span_start = match node.kind() {
                    AstKind::Function(f) => f.span.start,
                    AstKind::ArrowFunctionExpression(a) => a.span.start,
                    _ => continue,
                };
                let (line, column) =
                    byte_offset_to_line_col(ctx.source, span_start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: "Server action must be `async`. This function \
                              contains `\"use server\"` but is not async."
                        .into(),
                    severity: Severity::Error,
                    span: None,
                });
            }
        }

        diagnostics
    }
}
