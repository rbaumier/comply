//! no-identical-title OXC backend — flag repeated describe/test/it titles
//! within the same lexical scope.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::*;
use rustc_hash::FxHashSet;
use std::sync::Arc;

pub struct Check;

const TEST_BASES: &[&str] = &["describe", "test", "it"];

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[]
    }

    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for node in semantic.nodes().iter() {
            if let AstKind::Program(program) = node.kind() {
                check_statements(&program.body, ctx, &mut diagnostics);
            }
        }
        diagnostics
    }
}

/// Extract the base test construct name from a call expression callee.
/// Returns the base kind for `describe`, `test`, `it` (including `.only`/`.skip` variants).
fn classify_callee(expr: &Expression) -> Option<&'static str> {
    match expr {
        Expression::Identifier(id) => {
            TEST_BASES.iter().copied().find(|b| *b == id.name.as_str())
        }
        Expression::StaticMemberExpression(member) => {
            if let Expression::Identifier(obj) = &member.object {
                TEST_BASES.iter().copied().find(|b| *b == obj.name.as_str())
            } else {
                None
            }
        }
        _ => None,
    }
}

/// Extract a static string title from the first argument of a call.
fn static_title(args: &[Argument]) -> Option<String> {
    let first = args.first()?;
    match first {
        Argument::StringLiteral(s) => Some(s.value.to_string()),
        Argument::TemplateLiteral(t) => {
            if !t.expressions.is_empty() {
                return None;
            }
            let mut out = String::new();
            for quasi in &t.quasis {
                out.push_str(quasi.value.raw.as_str());
            }
            Some(out)
        }
        _ => None,
    }
}

/// Walk the direct statement children of a scope, tracking test titles by
/// construct kind. Recurse into describe callback bodies.
fn check_statements(
    stmts: &[Statement],
    ctx: &CheckCtx,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let mut seen: FxHashSet<(&'static str, String)> = FxHashSet::default();

    for stmt in stmts {
        let Statement::ExpressionStatement(expr_stmt) = stmt else {
            continue;
        };
        let Expression::CallExpression(call) = &expr_stmt.expression else {
            continue;
        };

        let Some(kind) = classify_callee(&call.callee) else {
            continue;
        };
        let Some(title) = static_title(&call.arguments) else {
            continue;
        };

        let key = (kind, title.clone());
        if !seen.insert(key) {
            let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: "no-identical-title".into(),
                message: format!(
                    "Duplicate {kind} title {title:?} in the same scope — use a unique title."
                ),
                severity: Severity::Warning,
                span: None,
            });
        }

        // For describe blocks, recurse into the callback body.
        if kind == "describe"
            && let Some(last_arg) = call.arguments.last() {
                let cb = match last_arg {
                    Argument::ArrowFunctionExpression(f) => Some(&f.body),
                    Argument::FunctionExpression(f) => f.body.as_ref(),
                    _ => None,
                };
                if let Some(body) = cb {
                    check_statements(&body.statements, ctx, diagnostics);
                }
            }
    }
}
