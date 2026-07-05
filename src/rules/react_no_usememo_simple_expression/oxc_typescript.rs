//! OxcCheck backend for react-no-usememo-simple-expression.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Argument, Expression, Statement, UnaryOperator};
use std::sync::Arc;

pub struct Check;

fn is_simple_expression(expr: &Expression) -> bool {
    match expr {
        Expression::Identifier(_)
        | Expression::NumericLiteral(_)
        | Expression::StringLiteral(_)
        | Expression::BooleanLiteral(_)
        | Expression::NullLiteral(_) => true,

        Expression::TemplateLiteral(tpl) => {
            tpl.expressions.iter().all(|e| is_simple_expression(e))
        }

        Expression::BinaryExpression(bin) => {
            is_simple_expression(&bin.left) && is_simple_expression(&bin.right)
        }

        Expression::UnaryExpression(unary) => {
            if matches!(
                unary.operator,
                UnaryOperator::Delete | UnaryOperator::Void
            ) {
                return false;
            }
            is_simple_expression(&unary.argument)
        }

        Expression::StaticMemberExpression(member) => is_simple_expression(&member.object),

        // Computed member (e.g. arr[index]) is NOT simple
        Expression::ComputedMemberExpression(_) => false,

        Expression::ConditionalExpression(cond) => {
            is_simple_expression(&cond.test)
                && is_simple_expression(&cond.consequent)
                && is_simple_expression(&cond.alternate)
        }

        Expression::ParenthesizedExpression(paren) => is_simple_expression(&paren.expression),

        Expression::TSAsExpression(as_expr) => is_simple_expression(&as_expr.expression),
        Expression::TSNonNullExpression(nn) => is_simple_expression(&nn.expression),
        Expression::TSSatisfiesExpression(sat) => is_simple_expression(&sat.expression),

        _ => false,
    }
}

fn is_usememo_call(
    call: &oxc_ast::ast::CallExpression,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    match &call.callee {
        // A bare `useMemo(...)` is React's only when the binding resolves to a
        // named import from `react`/`react-dom`. A same-named hook from another
        // module (e.g. `vooks`'s Vue reactive memo) or a local declaration is not
        // React's render-time memo, so the overhead rationale does not apply.
        Expression::Identifier(id) => {
            id.name.as_str() == "useMemo"
                && crate::oxc_helpers::is_imported_from_react("useMemo", semantic)
        }
        // The namespaced `React.useMemo(...)` form is already React-scoped.
        Expression::StaticMemberExpression(member) => {
            if let Expression::Identifier(obj) = &member.object {
                obj.name.as_str() == "React" && member.property.name.as_str() == "useMemo"
            } else {
                false
            }
        }
        _ => false,
    }
}

fn return_expr_from_body<'a>(
    body: &'a oxc_ast::ast::FunctionBody<'a>,
    is_expression: bool,
) -> Option<&'a Expression<'a>> {
    if is_expression {
        body.statements.first().and_then(|s| {
            if let Statement::ExpressionStatement(es) = s {
                Some(&es.expression)
            } else {
                None
            }
        })
    } else {
        if body.statements.len() != 1 {
            return None;
        }
        if let Statement::ReturnStatement(ret) = &body.statements[0] {
            ret.argument.as_ref()
        } else {
            None
        }
    }
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["useMemo"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else {
            return;
        };
        if !is_usememo_call(call, semantic) {
            return;
        }

        let Some(first_arg) = call.arguments.first() else {
            return;
        };

        let ret_expr = match first_arg {
            Argument::ArrowFunctionExpression(arrow) => {
                return_expr_from_body(&arrow.body, arrow.expression)
            }
            Argument::FunctionExpression(func) => {
                func.body.as_ref().and_then(|b| return_expr_from_body(b, false))
            }
            _ => return,
        };
        let Some(ret_expr) = ret_expr else {
            return;
        };
        if !is_simple_expression(ret_expr) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`useMemo` wrapping a trivially cheap expression — memo overhead exceeds the computation.".into(),
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

    // Regression for #7281: `useMemo` imported from `vooks` is a Vue reactive
    // memo (a getter is its required contract), not React's render-time hook, so
    // a trivially cheap body must not be flagged.
    #[test]
    fn skips_usememo_imported_from_vooks() {
        let src = "import { useMemo } from 'vooks';\n\
                   const emptyRef = useMemo(() => paginatedDataRef.value.length === 0);";
        assert!(run(src).is_empty());
    }

    // A bare `useMemo` that resolves to no react import (unresolvable binding)
    // does not fire.
    #[test]
    fn skips_unresolved_bare_usememo() {
        assert!(run("const x = useMemo(() => a.length === 0, []);").is_empty());
    }

    // A locally declared `useMemo` is not React's and does not fire.
    #[test]
    fn skips_local_usememo() {
        let src = "function useMemo(fn) { return fn(); }\n\
                   const x = useMemo(() => a.length === 0, []);";
        assert!(run(src).is_empty());
    }

    // React's `useMemo` wrapping a simple expression stays flagged.
    #[test]
    fn flags_react_usememo_simple_expression() {
        let src = "import { useMemo } from 'react';\n\
                   const x = useMemo(() => a.length === 0, []);";
        assert_eq!(run(src).len(), 1);
    }

    // A simple identifier memo from react stays flagged.
    #[test]
    fn flags_simple_identifier_from_react() {
        let src = "import { useMemo } from 'react';\n\
                   const x = useMemo(() => value, [value]);";
        assert_eq!(run(src).len(), 1);
    }

    // The namespaced `React.useMemo(...)` form stays flagged.
    #[test]
    fn flags_react_namespace_usememo() {
        let src = "import React from 'react';\n\
                   const x = React.useMemo(() => a.length === 0, []);";
        assert_eq!(run(src).len(), 1);
    }

    // A non-simple body (function call) from react is still allowed.
    #[test]
    fn allows_function_call_from_react() {
        let src = "import { useMemo } from 'react';\n\
                   const x = useMemo(() => compute(a, b), [a, b]);";
        assert!(run(src).is_empty());
    }
}
