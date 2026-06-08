//! xstate-no-async-guard oxc backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use oxc_span::GetSpan;
use std::sync::Arc;

fn is_async_expr(expr: &Expression) -> bool {
    match expr {
        Expression::ArrowFunctionExpression(arrow) => arrow.r#async,
        Expression::FunctionExpression(func) => func.r#async,
        _ => false,
    }
}

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::ObjectProperty]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::ObjectProperty(prop) = node.kind() else {
            return;
        };
        let key_text = match &prop.key {
            oxc_ast::ast::PropertyKey::StaticIdentifier(id) => id.name.as_str(),
            oxc_ast::ast::PropertyKey::StringLiteral(lit) => lit.value.as_str(),
            _ => return,
        };
        if key_text != "guard" && key_text != "cond" {
            return;
        }
        if !is_async_expr(&prop.value) {
            return;
        }

        let (line, column) =
            byte_offset_to_line_col(ctx.source, prop.value.span().start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "`{key_text}` must be synchronous — async guards return a Promise (always truthy). Use an actor for async logic."
            ),
            severity: Severity::Error,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;



    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
    }


    #[test]
    fn flags_async_arrow_guard() {
        let src = r#"
            const machine = createMachine({
                on: {
                    NEXT: {
                        target: 'b',
                        guard: async (ctx) => await isAllowed(ctx),
                    },
                },
            });
        "#;
        assert_eq!(run_on(src).len(), 1);
    }


    #[test]
    fn flags_async_function_expression_guard() {
        let src = r#"
            const machine = createMachine({
                on: {
                    NEXT: {
                        guard: async function (ctx) { return await check(ctx); },
                    },
                },
            });
        "#;
        assert_eq!(run_on(src).len(), 1);
    }


    #[test]
    fn flags_async_cond_legacy_api() {
        let src = r#"
            const machine = Machine({
                on: {
                    NEXT: {
                        cond: async (ctx) => await check(ctx),
                    },
                },
            });
        "#;
        assert_eq!(run_on(src).len(), 1);
    }


    #[test]
    fn allows_sync_arrow_guard() {
        let src = r#"
            const machine = createMachine({
                on: {
                    NEXT: { guard: (ctx) => ctx.count > 0 },
                },
            });
        "#;
        assert!(run_on(src).is_empty());
    }


    #[test]
    fn allows_sync_cond_guard() {
        let src = r#"
            const machine = createMachine({
                on: {
                    NEXT: { cond: function (ctx) { return ctx.count > 0; } },
                },
            });
        "#;
        assert!(run_on(src).is_empty());
    }


    #[test]
    fn allows_guard_reference_by_name() {
        let src = r#"
            const machine = createMachine({
                on: {
                    NEXT: { guard: 'isAllowed' },
                },
            });
        "#;
        assert!(run_on(src).is_empty());
    }


    #[test]
    fn ignores_unrelated_async_property() {
        let src = r#"
            const config = {
                handler: async (req) => await fetch(req),
            };
        "#;
        assert!(run_on(src).is_empty());
    }
}
