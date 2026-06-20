//! react-no-dedup-filter-indexof oxc backend.
//!
//! Matches the O(n²) dedup idiom `arr.filter((v, i) => arr.indexOf(v) === i)`
//! (equivalently `arr.filter((v, i, a) => a.indexOf(v) === i)`). Both halves of
//! the idiom are required: the `.indexOf()` receiver must be the *same* array
//! that is being filtered (its identifier, or the callback's array parameter),
//! and the call must be compared for equality against the callback's index
//! parameter. A membership check on a different array (`a.filter(x =>
//! b.indexOf(x) === -1)`) satisfies neither and is left alone.
//!
//! The same-array check matches the filtered receiver only when it is a bare
//! identifier or the callback's array parameter, so the rarer member-receiver
//! spelling `obj.list.filter((v, i) => obj.list.indexOf(v) === i)` is not
//! flagged — precision over recall for this FP-prone idiom.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{
    Argument, BinaryOperator, BindingPattern, Expression, FormalParameter, Statement,
};
use std::sync::Arc;

pub struct Check;

/// Identifiers that let us recognize the filtered array and the dedup index
/// inside the callback: the outer filtered-array identifier, plus the callback's
/// index (2nd) and array (3rd) parameter names.
struct DedupContext<'a> {
    /// Identifier of the array `.filter()` was called on, if it is a plain
    /// identifier (`arr` in `arr.filter(...)`).
    array_ident: Option<&'a str>,
    /// 2nd callback parameter name — the index (`i` in `(v, i) => ...`).
    index_param: Option<&'a str>,
    /// 3rd callback parameter name — the array (`a` in `(v, i, a) => ...`).
    array_param: Option<&'a str>,
}

fn param_identifier_name<'a>(param: &'a FormalParameter<'a>) -> Option<&'a str> {
    match &param.pattern {
        BindingPattern::BindingIdentifier(id) => Some(id.name.as_str()),
        _ => None,
    }
}

/// `obj` is the receiver of the matched `.indexOf()` — true when it names the
/// same array that is being filtered.
fn receiver_is_filtered_array(obj: &Expression, ctx: &DedupContext) -> bool {
    let Expression::Identifier(ident) = obj else {
        return false;
    };
    let name = ident.name.as_str();
    Some(name) == ctx.array_ident || Some(name) == ctx.array_param
}

/// `expr` is the operand compared against the `.indexOf()` call — true when it
/// is the callback's index parameter (the dedup signature), not `-1`/other.
fn operand_is_index_param(expr: &Expression, ctx: &DedupContext) -> bool {
    matches!(expr, Expression::Identifier(ident)
        if Some(ident.name.as_str()) == ctx.index_param)
}

/// True when `expr` is `<filtered-array>.indexOf(...)`.
fn is_filtered_array_indexof(expr: &Expression, ctx: &DedupContext) -> bool {
    let Expression::CallExpression(call) = expr else {
        return false;
    };
    let Expression::StaticMemberExpression(member) = &call.callee else {
        return false;
    };
    member.property.name.as_str() == "indexOf" && receiver_is_filtered_array(&member.object, ctx)
}

/// Detect the dedup comparison `<arr>.indexOf(v) === i` (or `i === <arr>.indexOf(v)`).
fn is_dedup_comparison(expr: &Expression, ctx: &DedupContext) -> bool {
    let Expression::BinaryExpression(bin) = expr else {
        return false;
    };
    if !matches!(bin.operator, BinaryOperator::StrictEquality | BinaryOperator::Equality) {
        return false;
    }
    (is_filtered_array_indexof(&bin.left, ctx) && operand_is_index_param(&bin.right, ctx))
        || (is_filtered_array_indexof(&bin.right, ctx) && operand_is_index_param(&bin.left, ctx))
}

/// Walk an expression tree looking for the dedup comparison.
fn expr_contains_dedup(expr: &Expression, ctx: &DedupContext) -> bool {
    if is_dedup_comparison(expr, ctx) {
        return true;
    }
    match expr {
        Expression::BinaryExpression(bin) => {
            expr_contains_dedup(&bin.left, ctx) || expr_contains_dedup(&bin.right, ctx)
        }
        Expression::LogicalExpression(log) => {
            expr_contains_dedup(&log.left, ctx) || expr_contains_dedup(&log.right, ctx)
        }
        Expression::UnaryExpression(un) => expr_contains_dedup(&un.argument, ctx),
        Expression::ParenthesizedExpression(paren) => expr_contains_dedup(&paren.expression, ctx),
        Expression::ConditionalExpression(cond) => {
            expr_contains_dedup(&cond.test, ctx)
                || expr_contains_dedup(&cond.consequent, ctx)
                || expr_contains_dedup(&cond.alternate, ctx)
        }
        Expression::CallExpression(call) => {
            call.arguments.iter().any(|arg| arg_contains_dedup(arg, ctx))
        }
        _ => false,
    }
}

fn arg_contains_dedup(arg: &Argument, ctx: &DedupContext) -> bool {
    match arg {
        Argument::SpreadElement(spread) => expr_contains_dedup(&spread.argument, ctx),
        _ => arg.as_expression().is_some_and(|e| expr_contains_dedup(e, ctx)),
    }
}

fn stmt_contains_dedup(stmt: &Statement, ctx: &DedupContext) -> bool {
    match stmt {
        Statement::ExpressionStatement(es) => expr_contains_dedup(&es.expression, ctx),
        Statement::ReturnStatement(ret) => {
            ret.argument.as_ref().is_some_and(|a| expr_contains_dedup(a, ctx))
        }
        Statement::IfStatement(if_stmt) => {
            expr_contains_dedup(&if_stmt.test, ctx)
                || stmt_contains_dedup(&if_stmt.consequent, ctx)
                || if_stmt.alternate.as_ref().is_some_and(|alt| stmt_contains_dedup(alt, ctx))
        }
        Statement::BlockStatement(block) => block.body.iter().any(|s| stmt_contains_dedup(s, ctx)),
        Statement::VariableDeclaration(decl) => decl.declarations.iter().any(|d| {
            d.init.as_ref().is_some_and(|init| expr_contains_dedup(init, ctx))
        }),
        _ => false,
    }
}

/// True when the callback body contains the dedup comparison against `ctx`.
fn callback_body_has_dedup(cb: &Expression, ctx: &DedupContext) -> bool {
    match cb {
        Expression::ArrowFunctionExpression(arrow) => {
            if arrow.expression
                && let Some(Statement::ExpressionStatement(es)) = arrow.body.statements.first()
            {
                return expr_contains_dedup(&es.expression, ctx);
            }
            arrow.body.statements.iter().any(|s| stmt_contains_dedup(s, ctx))
        }
        Expression::FunctionExpression(func) => func
            .body
            .as_ref()
            .is_some_and(|b| b.statements.iter().any(|s| stmt_contains_dedup(s, ctx))),
        _ => false,
    }
}

/// Extract the index (2nd) and array (3rd) parameter names from the callback.
fn callback_params<'a>(cb: &'a Expression<'a>) -> (Option<&'a str>, Option<&'a str>) {
    let params = match cb {
        Expression::ArrowFunctionExpression(arrow) => &arrow.params,
        Expression::FunctionExpression(func) => &func.params,
        _ => return (None, None),
    };
    let index_param = params.items.get(1).and_then(param_identifier_name);
    let array_param = params.items.get(2).and_then(param_identifier_name);
    (index_param, array_param)
}

impl OxcCheck for Check {
    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["filter"])
    }

    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else {
            return;
        };

        // Callee must be `<expr>.filter`.
        let Expression::StaticMemberExpression(member) = &call.callee else {
            return;
        };
        if member.property.name.as_str() != "filter" {
            return;
        }

        // Find arrow_function or function_expression in arguments.
        let Some(cb) = call.arguments.iter().find_map(|arg| {
            let expr = arg.as_expression()?;
            match expr {
                Expression::ArrowFunctionExpression(_) | Expression::FunctionExpression(_) => {
                    Some(expr)
                }
                _ => None,
            }
        }) else {
            return;
        };

        let array_ident = match &member.object {
            Expression::Identifier(ident) => Some(ident.name.as_str()),
            _ => None,
        };
        let (index_param, array_param) = callback_params(cb);
        let dedup_ctx = DedupContext { array_ident, index_param, array_param };

        // Without an index parameter the dedup signature `=== i` is impossible.
        if dedup_ctx.index_param.is_none() {
            return;
        }

        if !callback_body_has_dedup(cb, &dedup_ctx) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`.filter(... indexOf ...)` is O(n²) dedup — use `[...new Set(arr)]` (O(n))."
                .into(),
            severity: Severity::Warning,
            span: None,
        });
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

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.tsx")
    }

    #[test]
    fn flags_dedup_filter_indexof_third_param() {
        // Classic dedup: `a` is the 3rd callback param, compared `=== i`.
        let src = r#"const u = arr.filter((v, i, a) => a.indexOf(v) === i);"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_dedup_filter_indexof_outer_array() {
        // Dedup referencing the outer array identifier instead of the param.
        let src = r#"const u = arr.filter((v, i) => arr.indexOf(v) === i);"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_dedup_index_on_left() {
        let src = r#"const u = arr.filter((v, i, a) => i === a.indexOf(v));"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_set_dedup() {
        let src = r#"const u = [...new Set(arr)];"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_unrelated_filter() {
        let src = r#"const u = arr.filter(x => x > 0);"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_membership_check_different_array() {
        // #4993: filtering `a` by membership in a *different* array `b` against
        // `-1` is a set-difference, not dedup — must not flag.
        let src = r#"const u = a.filter(x => b.indexOf(x) === -1);"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_intersection_check_different_array() {
        // #4993: intersection (`!== -1`) on a different array — not dedup.
        let src = r#"const u = a.filter(x => b.indexOf(x) !== -1);"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_issue_block_body_membership() {
        // #4993 exact shape: filter `room.users`, membership-test the *other*
        // array `room.typingUsers` against `-1` inside a block body.
        let src = r#"
            const typingUsers = room.users.filter(user => {
              if (user._id === currentUserId) return;
              if (room.typingUsers.indexOf(user._id) === -1) return;
              return true;
            });
        "#;
        assert!(run(src).is_empty());
    }
}
