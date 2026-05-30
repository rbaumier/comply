//! cognitive-complexity OXC backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::*;
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

/// Recursively compute cognitive complexity of an AST subtree.
/// Uses the source text and oxc_parser to walk the tree, mirroring
/// the SonarSource spec.
fn compute_stmt(stmt: &Statement, nesting: u32) -> u32 {
    match stmt {
        Statement::IfStatement(if_stmt) => compute_if(if_stmt, nesting),
        Statement::ForStatement(f) => {
            1 + nesting + compute_body(&f.body, nesting + 1)
        }
        Statement::ForInStatement(f) => {
            1 + nesting + compute_body(&f.body, nesting + 1)
        }
        Statement::ForOfStatement(f) => {
            1 + nesting + compute_body(&f.body, nesting + 1)
        }
        Statement::WhileStatement(w) => {
            1 + nesting + compute_body(&w.body, nesting + 1)
        }
        Statement::DoWhileStatement(d) => {
            1 + nesting + compute_body(&d.body, nesting + 1)
        }
        Statement::SwitchStatement(sw) => {
            // The switch itself adds +1 (+ nesting penalty), but its case arms
            // do NOT get an extra nesting increment. Exhaustive switches enumerate
            // predetermined paths; penalising each arm's body with deeper nesting
            // produces FPs on domain error-mappers and locale maps.
            let mut score = 1 + nesting;
            for case in &sw.cases {
                for s in &case.consequent {
                    score += compute_stmt(s, nesting);
                }
            }
            score
        }
        Statement::TryStatement(t) => {
            let mut score = compute_stmts(&t.block.body, nesting);
            if let Some(ref handler) = t.handler {
                score += 1 + nesting; // catch clause
                score += compute_stmts(&handler.body.body, nesting + 1);
            }
            if let Some(ref finalizer) = t.finalizer {
                score += compute_stmts(&finalizer.body, nesting);
            }
            score
        }
        Statement::BlockStatement(block) => compute_stmts(&block.body, nesting),
        Statement::ReturnStatement(ret) => {
            ret.argument.as_ref().map_or(0, |e| compute_expr(e, nesting))
        }
        Statement::ExpressionStatement(es) => compute_expr(&es.expression, nesting),
        Statement::VariableDeclaration(decl) => {
            let mut score = 0;
            for d in &decl.declarations {
                if let Some(ref init) = d.init {
                    score += compute_expr(init, nesting);
                }
            }
            score
        }
        Statement::ThrowStatement(t) => compute_expr(&t.argument, nesting),
        _ => 0,
    }
}

fn compute_stmts(stmts: &[Statement], nesting: u32) -> u32 {
    stmts.iter().map(|s| compute_stmt(s, nesting)).sum()
}

fn compute_if(if_stmt: &IfStatement, nesting: u32) -> u32 {
    let mut score = 1 + nesting;
    score += compute_body(&if_stmt.consequent, nesting + 1);
    score += compute_expr(&if_stmt.test, nesting);
    if let Some(ref alt) = if_stmt.alternate {
        match alt {
            Statement::IfStatement(chained_if) => {
                // `else if` — count as 1 (no nesting penalty), don't increase nesting
                score += compute_if(chained_if, nesting);
            }
            _ => {
                // bare `else` — +1 only
                score += 1;
                score += compute_stmt(alt, nesting + 1);
            }
        }
    }
    score
}

fn compute_body(stmt: &Statement, nesting: u32) -> u32 {
    match stmt {
        Statement::BlockStatement(block) => compute_stmts(&block.body, nesting),
        _ => compute_stmt(stmt, nesting),
    }
}

fn compute_expr(expr: &Expression, nesting: u32) -> u32 {
    match expr {
        Expression::ConditionalExpression(ternary) => {
            let mut score = 1 + nesting;
            score += compute_expr(&ternary.test, nesting);
            score += compute_expr(&ternary.consequent, nesting + 1);
            score += compute_expr(&ternary.alternate, nesting + 1);
            score
        }
        Expression::LogicalExpression(logical) => {
            let mut score = 1; // +1 for logical operator
            score += compute_expr(&logical.left, nesting);
            score += compute_expr(&logical.right, nesting);
            score
        }
        Expression::ArrowFunctionExpression(_) | Expression::FunctionExpression(_) => {
            // Don't recurse into nested functions
            0
        }
        Expression::CallExpression(call) => {
            let mut score = 0;
            for arg in &call.arguments {
                if let Some(expr) = arg.as_expression() {
                    // Skip nested functions
                    if matches!(
                        expr,
                        Expression::ArrowFunctionExpression(_) | Expression::FunctionExpression(_)
                    ) {
                        continue;
                    }
                    score += compute_expr(expr, nesting);
                }
            }
            score
        }
        Expression::AssignmentExpression(assign) => {
            compute_expr(&assign.right, nesting)
        }
        Expression::ParenthesizedExpression(p) => {
            compute_expr(&p.expression, nesting)
        }
        _ => 0,
    }
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::Function, AstType::ArrowFunctionExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let threshold =
            ctx.config.threshold("cognitive-complexity", "max", ctx.lang) as u32;

        let complexity = match node.kind() {
            AstKind::Function(func) => {
                let Some(ref body) = func.body else { return };
                compute_stmts(&body.statements, 0)
            }
            AstKind::ArrowFunctionExpression(arrow) => {
                if arrow.expression {
                    return; // Concise arrow — negligible complexity
                }
                compute_stmts(&arrow.body.statements, 0)
            }
            _ => return,
        };

        if complexity > threshold {
            let span = node.kind().span();
            let (line, column) = byte_offset_to_line_col(ctx.source, span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: format!(
                    "Cognitive complexity is {complexity} (threshold {threshold}). Simplify this function."
                ),
                severity: Severity::Error,
                span: None,
            });
        }
    }
}
