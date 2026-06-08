//! prefer-array-index-of OxcCheck backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

pub struct Check;

const METHODS: &[&str] = &["findIndex", "findLastIndex"];

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["findIndex", "findLastIndex"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else { return };

        let Expression::StaticMemberExpression(member) = &call.callee else { return };
        let method = member.property.name.as_str();
        if !METHODS.contains(&method) {
            return;
        }

        // Must have exactly one argument that is an arrow function.
        if call.arguments.len() != 1 {
            return;
        }
        let Some(arg) = call.arguments.first() else { return };
        let arg_expr = match arg {
            oxc_ast::ast::Argument::ArrowFunctionExpression(arrow) => {
                check_arrow(arrow, ctx, call)
            }
            _ => return,
        };
        if !arg_expr {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Prefer `.indexOf(val)` over `.findIndex(x => x === val)`.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

fn check_arrow(
    arrow: &oxc_ast::ast::ArrowFunctionExpression,
    ctx: &CheckCtx,
    _call: &oxc_ast::ast::CallExpression,
) -> bool {
    // Must have exactly one parameter.
    if arrow.params.items.len() != 1 {
        return false;
    }
    let param = &arrow.params.items[0];
    let param_span = param.span;
    let param_name = &ctx.source[param_span.start as usize..param_span.end as usize];
    if param_name.is_empty() {
        return false;
    }

    // Body must be a simple `param === val` or `val === param` expression.
    if arrow.expression {
        // Concise body — single expression.
        if arrow.body.statements.len() != 1 {
            return false;
        }
        let oxc_ast::ast::Statement::ExpressionStatement(stmt) = &arrow.body.statements[0] else {
            return false;
        };
        return is_simple_equality(&stmt.expression, param_name, ctx);
    }
    // Block body with single return.
    let stmts: Vec<_> = arrow
        .body
        .statements
        .iter()
        .filter(|s| !matches!(s, oxc_ast::ast::Statement::EmptyStatement(_)))
        .collect();
    if stmts.len() != 1 {
        return false;
    }
    let oxc_ast::ast::Statement::ReturnStatement(ret) = stmts[0] else {
        return false;
    };
    let Some(arg) = &ret.argument else { return false };
    is_simple_equality(arg, param_name, ctx)
}

fn is_simple_equality(
    expr: &Expression,
    param_name: &str,
    ctx: &CheckCtx,
) -> bool {
    let Expression::BinaryExpression(bin) = expr else { return false };
    if bin.operator != oxc_ast::ast::BinaryOperator::StrictEquality {
        return false;
    }
    use oxc_span::GetSpan;
    let left_text = &ctx.source[bin.left.span().start as usize..bin.left.span().end as usize];
    let right_text = &ctx.source[bin.right.span().start as usize..bin.right.span().end as usize];

    let left_is_ident = matches!(&bin.left, Expression::Identifier(_));
    let right_is_ident = matches!(&bin.right, Expression::Identifier(_));

    (left_text == param_name && left_is_ident && right_is_ident)
        || (right_text == param_name && right_is_ident && left_is_ident)
}

#[cfg(test)]
mod tests {
    use super::*;



    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
    }


    #[test]
    fn flags_findindex_arrow_equality() {
        assert_eq!(run_on("const i = arr.findIndex(x => x === val);").len(), 1);
    }


    #[test]
    fn flags_findindex_parens_arrow() {
        assert_eq!(
            run_on("const i = arr.findIndex((x) => x === val);").len(),
            1
        );
    }


    #[test]
    fn flags_findindex_reversed_comparison() {
        assert_eq!(run_on("const i = arr.findIndex(x => val === x);").len(), 1);
    }


    #[test]
    fn flags_findlastindex() {
        assert_eq!(
            run_on("const i = arr.findLastIndex(x => x === val);").len(),
            1
        );
    }


    #[test]
    fn allows_indexof() {
        assert!(run_on("const i = arr.indexOf(val);").is_empty());
    }


    #[test]
    fn allows_complex_callback() {
        assert!(run_on("const i = arr.findIndex(x => x.id === val);").is_empty());
    }
}
