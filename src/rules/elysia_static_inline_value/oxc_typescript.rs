//! elysia-static-inline-value OXC backend — flag arrow handlers that only
//! return a string literal.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Argument, Expression, FormalParameterKind, Statement};
use std::sync::Arc;

pub struct Check;

const ROUTE_METHODS: &[&str] = &[
    "get", "post", "put", "patch", "delete", "all", "head", "options",
];

fn is_string_literal(expr: &Expression) -> bool {
    matches!(expr, Expression::StringLiteral(_) | Expression::TemplateLiteral(_))
}

fn arrow_returns_only_string(arrow: &oxc_ast::ast::ArrowFunctionExpression) -> bool {
    // Expression body: `() => "literal"`
    if arrow.expression {
        let Some(Statement::ExpressionStatement(stmt)) = arrow.body.statements.first() else {
            return false;
        };
        return is_string_literal(&stmt.expression);
    }
    // Block body with a single return statement.
    let stmts: Vec<_> = arrow.body.statements.iter().collect();
    if stmts.len() != 1 {
        return false;
    }
    let Statement::ReturnStatement(ret) = stmts[0] else { return false };
    ret.argument.as_ref().is_some_and(|arg| is_string_literal(arg))
}

impl OxcCheck for Check {
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
        if !ctx.project.has_framework("elysia") {
            return;
        }

        let AstKind::CallExpression(call) = node.kind() else { return };

        let Expression::StaticMemberExpression(member) = &call.callee else { return };
        if !ROUTE_METHODS.contains(&member.property.name.as_str()) {
            return;
        }

        if call.arguments.len() < 2 {
            return;
        }
        let Argument::ArrowFunctionExpression(arrow) = &call.arguments[1] else {
            return;
        };

        // Bail if the arrow takes any parameters.
        if arrow.params.kind == FormalParameterKind::FormalParameter
            && !arrow.params.items.is_empty()
        {
            return;
        }

        if !arrow_returns_only_string(arrow) {
            return;
        }

        let (line, column) =
            byte_offset_to_line_col(ctx.source, arrow.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Handler returns only a static string \u{2014} pass the literal directly so Elysia can compile it ahead of time.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;



    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts_with_framework(source, &Check, "elysia")
    }


    #[test]
    fn flags_arrow_returning_string_literal() {
        let src = "import { Elysia } from 'elysia';\napp.get('/health', () => 'ok');";
        assert_eq!(run_on(src).len(), 1);
    }


    #[test]
    fn flags_arrow_with_block_returning_string() {
        let src = "import { Elysia } from 'elysia';\napp.get('/health', () => { return 'ok'; });";
        assert_eq!(run_on(src).len(), 1);
    }


    #[test]
    fn allows_static_string_arg() {
        let src = "import { Elysia } from 'elysia';\napp.get('/health', 'ok');";
        assert!(run_on(src).is_empty());
    }


    #[test]
    fn ignores_non_elysia_files() {
        let src = "app.get('/health', () => 'ok');";
        assert!(crate::rules::test_helpers::run_oxc_ts(src, &Check).is_empty());
    }
}
