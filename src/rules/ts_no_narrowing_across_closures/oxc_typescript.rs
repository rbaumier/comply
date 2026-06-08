//! ts-no-narrowing-across-closures oxc backend.
//!
//! Inside an `if (x)` / `if (x !== null)` block, if a call to
//! `setTimeout`/`.then`/`.catch`/`addEventListener` takes a function
//! expression that references `x` directly (not captured by a local
//! const), flag it.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{
    BindingPattern, Expression, Statement,
};
use oxc_span::GetSpan;
use std::sync::Arc;

const CLOSURE_CALLEES: &[&str] = &[
    "setTimeout",
    "setInterval",
    "queueMicrotask",
    "requestAnimationFrame",
];
const CLOSURE_METHODS: &[&str] = &["then", "catch", "finally", "addEventListener"];

pub struct Check;

fn narrowed_identifier<'a>(expr: &'a Expression<'a>) -> Option<&'a str> {
    match expr {
        Expression::Identifier(id) => Some(id.name.as_str()),
        Expression::BinaryExpression(bin) => {
            if let Expression::Identifier(id) = &bin.left {
                Some(id.name.as_str())
            } else {
                None
            }
        }
        Expression::ParenthesizedExpression(paren) => narrowed_identifier(&paren.expression),
        _ => None,
    }
}

fn is_closure_call(callee: &Expression) -> bool {
    match callee {
        Expression::Identifier(id) => CLOSURE_CALLEES.contains(&id.name.as_str()),
        Expression::StaticMemberExpression(member) => {
            CLOSURE_METHODS.contains(&member.property.name.as_str())
        }
        _ => false,
    }
}

fn has_local_const(stmts: &oxc_allocator::Vec<'_, Statement<'_>>, name: &str) -> bool {
    for stmt in stmts.iter() {
        if let Statement::VariableDeclaration(vd) = stmt {
            if !vd.kind.is_const() {
                continue;
            }
            for decl in &vd.declarations {
                if let BindingPattern::BindingIdentifier(id) = &decl.id
                    && id.name.as_str() == name {
                        return true;
                    }
            }
        }
    }
    false
}

/// Check if a callback body source text references the identifier name.
/// Uses source-text substring check for simplicity — the identifier must
/// appear as a word boundary.
fn callback_body_references(source: &str, span: oxc_span::Span, name: &str) -> bool {
    let body_text = &source[span.start as usize..span.end as usize];
    // Simple word-boundary check: look for the name not preceded/followed by
    // alphanumeric or underscore.
    body_text.contains(name)
        && body_text.split(|c: char| !c.is_alphanumeric() && c != '_')
            .any(|word| word == name)
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::IfStatement]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::IfStatement(stmt) = node.kind() else {
            return;
        };
        let Some(name) = narrowed_identifier(&stmt.test) else {
            return;
        };
        let Statement::BlockStatement(block) = &stmt.consequent else {
            return;
        };
        if has_local_const(&block.body, name) {
            return;
        }

        // Walk the block body looking for closure-scheduling calls.
        visit_stmts(&block.body, name, ctx, diagnostics);
    }
}

fn visit_stmts(
    stmts: &oxc_allocator::Vec<'_, Statement<'_>>,
    name: &str,
    ctx: &CheckCtx,
    diagnostics: &mut Vec<Diagnostic>,
) {
    for stmt in stmts.iter() {
        visit_stmt(stmt, name, ctx, diagnostics);
    }
}

fn visit_stmt(stmt: &Statement, name: &str, ctx: &CheckCtx, diagnostics: &mut Vec<Diagnostic>) {
    match stmt {
        Statement::ExpressionStatement(es) => {
            visit_expr(&es.expression, name, ctx, diagnostics);
        }
        Statement::BlockStatement(block) => {
            visit_stmts(&block.body, name, ctx, diagnostics);
        }
        Statement::IfStatement(ifs) => {
            visit_stmt(&ifs.consequent, name, ctx, diagnostics);
            if let Some(alt) = &ifs.alternate {
                visit_stmt(alt, name, ctx, diagnostics);
            }
        }
        Statement::VariableDeclaration(vd) => {
            for decl in &vd.declarations {
                if let Some(init) = &decl.init {
                    visit_expr(init, name, ctx, diagnostics);
                }
            }
        }
        _ => {}
    }
}

fn visit_expr(expr: &Expression, name: &str, ctx: &CheckCtx, diagnostics: &mut Vec<Diagnostic>) {
    let Expression::CallExpression(call) = expr else {
        return;
    };
    if !is_closure_call(&call.callee) {
        return;
    }
    for arg in &call.arguments {
        let Some(arg_expr) = arg.as_expression() else {
            continue;
        };
        let (is_callback, body_span, cb_span) = match arg_expr {
            Expression::ArrowFunctionExpression(arrow) => {
                (true, arrow.body.span, arrow.span)
            }
            Expression::FunctionExpression(func) => {
                if let Some(body) = &func.body {
                    (true, body.span, func.span())
                } else {
                    continue;
                }
            }
            _ => continue,
        };
        if !is_callback {
            continue;
        }
        if callback_body_references(ctx.source, body_span, name) {
            let (line, column) = byte_offset_to_line_col(ctx.source, cb_span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: format!(
                    "Variable `{name}` loses its narrowing inside this callback; capture it in a local const first."
                ),
                severity: Severity::Warning,
                span: None,
            });
        }
    }
}

#[cfg(test)]
impl crate::rules::test_helpers::RunRule for Check {
    fn meta(&self) -> &'static crate::rules::meta::RuleMeta {
        &super::META
    }
    fn execute_with_ctx(
        &self,
        src: &str,
        path: &std::path::Path,
        project: &crate::project::ProjectCtx,
        file: &crate::rules::file_ctx::FileCtx,
    ) -> Vec<crate::diagnostic::Diagnostic> {
        crate::rules::test_helpers::run_oxc_check(self, src, path, project, file)
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, s, "t.ts")
    }

    #[test]
    fn flags_set_timeout_using_narrowed_var() {
        let src = "function f(user: { name: string } | null) { if (user) { setTimeout(() => console.log(user.name), 0); } }";
        let diags = run(src);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn flags_promise_then() {
        let src = "function f(u: string | null) { if (u) { p.then(() => console.log(u)); } }";
        let diags = run(src);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn allows_local_const_capture() {
        let src = "function f(user: { name: string } | null) { if (user) { const user = user; setTimeout(() => console.log(user.name), 0); } }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_plain_usage_without_closure() {
        let src =
            "function f(user: { name: string } | null) { if (user) { console.log(user.name); } }";
        assert!(run(src).is_empty());
    }
}
