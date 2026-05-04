//! OxcCheck backend for tanstack-start-no-window-in-render.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{
    Argument, BindingPattern, Expression, Statement,
};
use std::sync::Arc;

const SAFE_CALLBACK_HOOKS: &[&str] = &[
    "useEffect",
    "useLayoutEffect",
    "useCallback",
    "useMemo",
    "useImperativeHandle",
];

pub struct Check;

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
            match node.kind() {
                AstKind::Function(func) => {
                    let name = func
                        .id
                        .as_ref()
                        .map(|id| id.name.as_str());
                    if !name.is_some_and(|n| n.starts_with(|c: char| c.is_ascii_uppercase())) {
                        continue;
                    }
                    if let Some(body) = &func.body {
                        scan_render_body(&body.statements, ctx, &mut diagnostics);
                    }
                }
                AstKind::VariableDeclarator(decl) => {
                    let BindingPattern::BindingIdentifier(id) = &decl.id else {
                        continue;
                    };
                    if !id.name.starts_with(|c: char| c.is_ascii_uppercase()) {
                        continue;
                    }
                    let Some(init) = &decl.init else {
                        continue;
                    };
                    match init {
                        Expression::ArrowFunctionExpression(arrow) => {
                            if !arrow.expression {
                                scan_render_body(
                                    &arrow.body.statements,
                                    ctx,
                                    &mut diagnostics,
                                );
                            }
                        }
                        Expression::FunctionExpression(func) => {
                            if let Some(body) = &func.body {
                                scan_render_body(
                                    &body.statements,
                                    ctx,
                                    &mut diagnostics,
                                );
                            }
                        }
                        _ => {}
                    }
                }
                _ => {}
            }
        }

        diagnostics
    }
}

fn scan_render_body(
    stmts: &[Statement],
    ctx: &CheckCtx,
    diagnostics: &mut Vec<Diagnostic>,
) {
    for stmt in stmts {
        scan_stmt(stmt, ctx, diagnostics);
    }
}

fn scan_stmt(
    stmt: &Statement,
    ctx: &CheckCtx,
    diagnostics: &mut Vec<Diagnostic>,
) {
    match stmt {
        Statement::ExpressionStatement(es) => {
            // Skip safe callback hooks
            if is_safe_callback_hook_expr(&es.expression) {
                return;
            }
            // Skip nested functions
            if matches!(
                es.expression,
                Expression::ArrowFunctionExpression(_) | Expression::FunctionExpression(_)
            ) {
                return;
            }
            scan_expr(&es.expression, ctx, diagnostics);
        }
        Statement::VariableDeclaration(decl) => {
            for declarator in &decl.declarations {
                if let Some(init) = &declarator.init {
                    // Skip if init is a safe callback hook
                    if is_safe_callback_hook_expr(init) {
                        continue;
                    }
                    // Skip nested functions
                    if matches!(
                        init,
                        Expression::ArrowFunctionExpression(_)
                            | Expression::FunctionExpression(_)
                    ) {
                        continue;
                    }
                    scan_expr(init, ctx, diagnostics);
                }
            }
        }
        Statement::ReturnStatement(ret) => {
            if let Some(arg) = &ret.argument {
                scan_expr(arg, ctx, diagnostics);
            }
        }
        Statement::IfStatement(if_stmt) => {
            scan_expr(&if_stmt.test, ctx, diagnostics);
            scan_stmt(&if_stmt.consequent, ctx, diagnostics);
            if let Some(alt) = &if_stmt.alternate {
                scan_stmt(alt, ctx, diagnostics);
            }
        }
        Statement::BlockStatement(block) => {
            for s in &block.body {
                scan_stmt(s, ctx, diagnostics);
            }
        }
        // Skip nested function declarations
        Statement::FunctionDeclaration(_) => {}
        _ => {}
    }
}

fn scan_expr(
    expr: &Expression,
    ctx: &CheckCtx,
    diagnostics: &mut Vec<Diagnostic>,
) {
    match expr {
        Expression::StaticMemberExpression(member) => {
            if let Some(name) = offending_member_name(&member.object) {
                if !is_guarded_by_typeof(ctx.source, member.span.start as usize, name) {
                    let (line, column) =
                        byte_offset_to_line_col(ctx.source, member.span.start as usize);
                    diagnostics.push(Diagnostic {
                        path: Arc::clone(&ctx.path_arc),
                        line,
                        column,
                        rule_id: super::META.id.into(),
                        message: format!(
                            "`{name}.*` in render breaks SSR. Read from `{name}` inside a \
                             `useEffect`, or guard with `typeof {name} !== 'undefined'`."
                        ),
                        severity: Severity::Warning,
                        span: None,
                    });
                }
                return;
            }
            // Skip safe callback hook calls in the callee chain
            if is_safe_callback_hook_expr(expr) {
                return;
            }
            scan_expr(&member.object, ctx, diagnostics);
        }
        Expression::ComputedMemberExpression(member) => {
            if let Some(name) = offending_member_name(&member.object) {
                if !is_guarded_by_typeof(ctx.source, member.span.start as usize, name) {
                    let (line, column) =
                        byte_offset_to_line_col(ctx.source, member.span.start as usize);
                    diagnostics.push(Diagnostic {
                        path: Arc::clone(&ctx.path_arc),
                        line,
                        column,
                        rule_id: super::META.id.into(),
                        message: format!(
                            "`{name}.*` in render breaks SSR. Read from `{name}` inside a \
                             `useEffect`, or guard with `typeof {name} !== 'undefined'`."
                        ),
                        severity: Severity::Warning,
                        span: None,
                    });
                }
                return;
            }
            scan_expr(&member.object, ctx, diagnostics);
            scan_expr(&member.expression, ctx, diagnostics);
        }
        Expression::CallExpression(call) => {
            if is_safe_callback_hook_expr(expr) {
                return;
            }
            scan_expr(&call.callee, ctx, diagnostics);
            for arg in &call.arguments {
                match arg {
                    Argument::SpreadElement(spread) => {
                        scan_expr(&spread.argument, ctx, diagnostics);
                    }
                    _ => {
                        if let Some(e) = arg.as_expression() {
                            // Don't descend into callback functions
                            if matches!(
                                e,
                                Expression::ArrowFunctionExpression(_)
                                    | Expression::FunctionExpression(_)
                            ) {
                                continue;
                            }
                            scan_expr(e, ctx, diagnostics);
                        }
                    }
                }
            }
        }
        Expression::AssignmentExpression(assign) => {
            scan_expr(&assign.right, ctx, diagnostics);
        }
        Expression::ConditionalExpression(cond) => {
            scan_expr(&cond.test, ctx, diagnostics);
            scan_expr(&cond.consequent, ctx, diagnostics);
            scan_expr(&cond.alternate, ctx, diagnostics);
        }
        Expression::BinaryExpression(bin) => {
            scan_expr(&bin.left, ctx, diagnostics);
            scan_expr(&bin.right, ctx, diagnostics);
        }
        Expression::LogicalExpression(log) => {
            scan_expr(&log.left, ctx, diagnostics);
            scan_expr(&log.right, ctx, diagnostics);
        }
        Expression::TemplateLiteral(tpl) => {
            for expr in &tpl.expressions {
                scan_expr(expr, ctx, diagnostics);
            }
        }
        Expression::ParenthesizedExpression(paren) => {
            scan_expr(&paren.expression, ctx, diagnostics);
        }
        // Skip nested arrow/function expressions
        Expression::ArrowFunctionExpression(_) | Expression::FunctionExpression(_) => {}
        _ => {}
    }
}

fn offending_member_name<'a>(obj: &'a Expression<'a>) -> Option<&'static str> {
    let Expression::Identifier(id) = obj else {
        return None;
    };
    match id.name.as_str() {
        "window" => Some("window"),
        "document" => Some("document"),
        _ => None,
    }
}

fn is_safe_callback_hook_expr(expr: &Expression) -> bool {
    let Expression::CallExpression(call) = expr else {
        return false;
    };
    let Expression::Identifier(id) = &call.callee else {
        return false;
    };
    SAFE_CALLBACK_HOOKS.contains(&id.name.as_str())
}

/// Check if the source location is inside a `typeof <name> !== "undefined"` guard.
/// We do a text-based check on the source up to the member expression.
fn is_guarded_by_typeof(source: &str, offset: usize, name: &str) -> bool {
    let needle_dq = format!("typeof {} !== \"undefined\"", name);
    let needle_sq = format!("typeof {} !== 'undefined'", name);

    // Look backwards from the offset for an enclosing if statement condition
    // containing the typeof guard.
    let prefix = &source[..offset];

    // Simple heuristic: look for the typeof guard pattern within the
    // recent preceding source (within reasonable distance).
    let search_window = &prefix[prefix.len().saturating_sub(500)..];
    let normalized: String = search_window.split_whitespace().collect::<Vec<_>>().join(" ");
    // Check that the guard appears and we're inside its block
    if normalized.contains(&needle_dq) || normalized.contains(&needle_sq) {
        // Verify we're inside the if-block, not after it.
        // Count opening/closing braces after the typeof guard to see if we're still inside.
        let guard_pos = if let Some(p) = normalized.rfind(&needle_dq) {
            Some(p)
        } else {
            normalized.rfind(&needle_sq)
        };
        if let Some(gp) = guard_pos {
            let after_guard = &normalized[gp..];
            let opens: usize = after_guard.matches('{').count();
            let closes: usize = after_guard.matches('}').count();
            if opens > closes {
                return true;
            }
        }
    }
    false
}
