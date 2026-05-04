//! no-assign-mutated-array OxcCheck backend — flag assignments whose RHS
//! is a mutating array method call (sort, reverse, fill).

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

const MUTATING_METHODS: &[&str] = &["sort", "reverse", "fill"];

/// Check if a call is a mutating array method and return the method name.
fn mutating_method_name<'a>(expr: &'a Expression<'a>, source: &str) -> Option<&'a str> {
    let call = unwrap_expr(expr);
    let Expression::CallExpression(call) = call else { return None };
    let Expression::StaticMemberExpression(member) = &call.callee else { return None };
    let name = member.property.name.as_str();
    if !MUTATING_METHODS.contains(&name) {
        return None;
    }

    // Allow when the receiver is a freshly-created array.
    if is_fresh_array(&member.object, source) {
        return None;
    }

    Some(name)
}

/// Walk through parenthesized / type assertion wrappers.
fn unwrap_expr<'a, 'b>(expr: &'b Expression<'a>) -> &'b Expression<'a> {
    match expr {
        Expression::ParenthesizedExpression(p) => unwrap_expr(&p.expression),
        Expression::TSAsExpression(t) => unwrap_expr(&t.expression),
        Expression::TSSatisfiesExpression(t) => unwrap_expr(&t.expression),
        Expression::TSNonNullExpression(t) => unwrap_expr(&t.expression),
        Expression::TSTypeAssertion(t) => unwrap_expr(&t.expression),
        _ => expr,
    }
}

fn is_fresh_array(expr: &Expression, source: &str) -> bool {
    match expr {
        Expression::ArrayExpression(_) => {
            // Spread copy: `[...arr]`
            let text = &source[expr.span().start as usize..expr.span().end as usize];
            text.contains("...")
        }
        Expression::CallExpression(call) => {
            let Expression::StaticMemberExpression(member) = &call.callee else {
                return false;
            };
            matches!(
                member.property.name.as_str(),
                "slice" | "filter" | "map" | "concat" | "flat" | "flatMap"
                    | "toSorted" | "toReversed" | "toSpliced" | "with"
            )
        }
        _ => false,
    }
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::VariableDeclaration, AstType::AssignmentExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&[".sort(", ".reverse(", ".fill("])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        match node.kind() {
            AstKind::VariableDeclaration(decl) => {
                for declarator in &decl.declarations {
                    let Some(init) = &declarator.init else { continue };
                    let Some(method) = mutating_method_name(init, ctx.source) else { continue };
                    let (line, column) =
                        byte_offset_to_line_col(ctx.source, init.span().start as usize);
                    diagnostics.push(Diagnostic {
                        path: Arc::clone(&ctx.path_arc),
                        line,
                        column,
                        rule_id: super::META.id.into(),
                        message: format!(
                            "Assigning result of `.{method}()` — mutating method returns the same array. \
                             Use `toSorted()`, `toReversed()`, or spread before mutating: `[...arr].{method}(...)`."
                        ),
                        severity: Severity::Warning,
                        span: None,
                    });
                }
            }
            AstKind::AssignmentExpression(assign) => {
                let Some(method) = mutating_method_name(&assign.right, ctx.source) else { return };
                let (line, column) =
                    byte_offset_to_line_col(ctx.source, assign.right.span().start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: format!(
                        "Assigning result of `.{method}()` — mutating method returns the same array. \
                         Use `toSorted()`, `toReversed()`, or spread before mutating: `[...arr].{method}(...)`."
                    ),
                    severity: Severity::Warning,
                    span: None,
                });
            }
            _ => {}
        }
    }
}
