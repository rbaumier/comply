//! tanstack-query-no-void-query-fn oxc backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Expression, ObjectPropertyKind, PropertyKey, Statement};
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

/// True if `body.statements` has at least one `return <value>` (or
/// terminal expression body for arrow with `expression: true`).
fn function_returns_value(arrow: &oxc_ast::ast::ArrowFunctionExpression) -> bool {
    if arrow.expression {
        // `() => value` — expression body always returns.
        return true;
    }
    body_has_return_with_value(&arrow.body.statements)
}

fn body_has_return_with_value(stmts: &[Statement]) -> bool {
    for stmt in stmts {
        match stmt {
            Statement::ReturnStatement(ret) if ret.argument.is_some() => return true,
            Statement::IfStatement(if_stmt) => {
                if statement_returns_value(&if_stmt.consequent) {
                    return true;
                }
                if let Some(alt) = &if_stmt.alternate
                    && statement_returns_value(alt)
                {
                    return true;
                }
            }
            Statement::BlockStatement(block) => {
                if body_has_return_with_value(&block.body) {
                    return true;
                }
            }
            Statement::TryStatement(try_stmt) => {
                if body_has_return_with_value(&try_stmt.block.body) {
                    return true;
                }
                if let Some(handler) = &try_stmt.handler
                    && body_has_return_with_value(&handler.body.body)
                {
                    return true;
                }
            }
            _ => {}
        }
    }
    false
}

fn statement_returns_value(stmt: &Statement) -> bool {
    body_has_return_with_value(std::slice::from_ref(stmt))
}

const QUERY_CALLEES: &[&str] = &[
    "useQuery",
    "useSuspenseQuery",
    "useInfiniteQuery",
    "useSuspenseInfiniteQuery",
    "queryOptions",
];

fn is_query_callee(call: &oxc_ast::ast::CallExpression) -> bool {
    match &call.callee {
        Expression::Identifier(id) => QUERY_CALLEES.contains(&id.name.as_str()),
        Expression::StaticMemberExpression(m) => {
            QUERY_CALLEES.contains(&m.property.name.as_str())
        }
        _ => false,
    }
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["queryFn"])
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
        if !is_query_callee(call) {
            return;
        }
        let Some(first_arg) = call.arguments.first() else {
            return;
        };
        let Some(Expression::ObjectExpression(obj)) = first_arg.as_expression() else {
            return;
        };
        for prop in &obj.properties {
            let ObjectPropertyKind::ObjectProperty(p) = prop else {
                continue;
            };
            let PropertyKey::StaticIdentifier(key) = &p.key else {
                continue;
            };
            if key.name.as_str() != "queryFn" {
                continue;
            }
            let Expression::ArrowFunctionExpression(arrow) = &p.value else {
                continue;
            };
            if function_returns_value(arrow) {
                continue;
            }
            let (line, column) = byte_offset_to_line_col(ctx.source, arrow.span().start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: "`queryFn` has no value-returning path — TanStack Query \
                          will cache `undefined` and every consumer's `data` will \
                          be undefined. Return the response value, or switch to \
                          `useMutation` if a side effect is the goal."
                    .into(),
                severity: Severity::Warning,
                span: None,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_tsx(src, &Check)
    }

    #[test]
    fn flags_query_fn_without_return() {
        let src = r#"
            const q = useQuery({
                queryKey: ["x"],
                queryFn: async () => { await fetch("/x"); },
            });
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_query_fn_with_return() {
        let src = r#"
            const q = useQuery({
                queryKey: ["x"],
                queryFn: async () => { const r = await fetch("/x"); return r.json(); },
            });
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_concise_arrow_body() {
        let src = r#"
            const q = useQuery({
                queryKey: ["x"],
                queryFn: () => fetch("/x").then(r => r.json()),
            });
        "#;
        assert!(run(src).is_empty());
    }
}
