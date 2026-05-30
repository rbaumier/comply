//! OXC backend for testing-no-concurrent-without-context-expect.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Argument, BindingPattern, Expression, FunctionBody, Statement};
use std::sync::Arc;

pub struct Check;

/// Returns true if the statement (or any non-function-boundary descendant)
/// contains a direct `expect(...)` call.
fn stmt_has_expect_call(stmt: &Statement) -> bool {
    match stmt {
        Statement::ExpressionStatement(es) => expr_has_expect_call(&es.expression),
        Statement::VariableDeclaration(decl) => decl
            .declarations
            .iter()
            .any(|d| d.init.as_ref().is_some_and(|e| expr_has_expect_call(e))),
        Statement::ReturnStatement(ret) => ret
            .argument
            .as_ref()
            .is_some_and(|e| expr_has_expect_call(e)),
        Statement::BlockStatement(block) => block.body.iter().any(stmt_has_expect_call),
        Statement::TryStatement(try_stmt) => {
            try_stmt.block.body.iter().any(stmt_has_expect_call)
                || try_stmt
                    .handler
                    .as_ref()
                    .is_some_and(|h| h.body.body.iter().any(stmt_has_expect_call))
                || try_stmt
                    .finalizer
                    .as_ref()
                    .is_some_and(|f| f.body.iter().any(stmt_has_expect_call))
        }
        Statement::IfStatement(if_stmt) => {
            stmt_has_expect_call(&if_stmt.consequent)
                || if_stmt
                    .alternate
                    .as_ref()
                    .is_some_and(|a| stmt_has_expect_call(a))
        }
        _ => false,
    }
}

/// Returns true if the expression or any non-function-boundary descendant
/// contains a direct `expect(...)` call.
fn expr_has_expect_call(expr: &Expression) -> bool {
    match expr {
        Expression::CallExpression(call) => {
            if let Expression::Identifier(id) = &call.callee {
                if id.name.as_str() == "expect" {
                    return true;
                }
            }
            // Recurse into callee for chained calls like `expect(x).toBe(y)`.
            expr_has_expect_call(&call.callee)
                || call.arguments.iter().any(|arg| {
                    // Don't descend into nested function definitions.
                    !matches!(
                        arg,
                        Argument::ArrowFunctionExpression(_) | Argument::FunctionExpression(_)
                    ) && arg
                        .as_expression()
                        .is_some_and(expr_has_expect_call)
                })
        }
        Expression::AwaitExpression(aw) => expr_has_expect_call(&aw.argument),
        Expression::StaticMemberExpression(me) => expr_has_expect_call(&me.object),
        Expression::ComputedMemberExpression(me) => expr_has_expect_call(&me.object),
        // Don't descend into nested function definitions.
        Expression::ArrowFunctionExpression(_) | Expression::FunctionExpression(_) => false,
        _ => false,
    }
}

fn body_has_expect_call(body: &FunctionBody) -> bool {
    body.statements.iter().any(stmt_has_expect_call)
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
        let AstKind::CallExpression(call) = node.kind() else {
            return;
        };

        // Callee must be `test.concurrent` or `it.concurrent`.
        let Expression::StaticMemberExpression(member) = &call.callee else {
            return;
        };
        if member.property.name.as_str() != "concurrent" {
            return;
        }
        let Expression::Identifier(obj) = &member.object else {
            return;
        };
        if !matches!(obj.name.as_str(), "test" | "it") {
            return;
        }

        // Find the callback argument (arrow function or function expression),
        // capturing both its params and body.
        let Some((params, body)) = call.arguments.iter().find_map(|arg| {
            let expr = arg.as_expression()?;
            match expr {
                Expression::ArrowFunctionExpression(f) => Some((&f.params, &*f.body)),
                Expression::FunctionExpression(f) => Some((&f.params, f.body.as_deref()?)),
                _ => None,
            }
        }) else {
            return;
        };

        // Check if the first parameter destructures `expect`.
        let has_expect_param = params.items.first().is_some_and(|param| {
            if let BindingPattern::ObjectPattern(obj_pat) = &param.pattern {
                obj_pat.properties.iter().any(|prop| {
                    let key_name = prop.key.name();
                    key_name.as_deref() == Some("expect")
                })
            } else {
                false
            }
        });

        if has_expect_param {
            return;
        }

        // Only flag when the callback body actually calls `expect(...)` directly.
        // If the body delegates entirely to an external function (e.g. a tx-test
        // helper wrapping `fn: () => Promise<void>`), there is nothing to fix —
        // the external fn's signature cannot receive the context expect.
        if !body_has_expect_call(body) {
            return;
        }

        let span = params.span;
        let (line, column) = byte_offset_to_line_col(ctx.source, span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "test.concurrent must destructure { expect } from the test context — the module-level expect is not scoped per concurrent test.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::test_helpers::run_oxc_ts;

    fn run(s: &str) -> Vec<Diagnostic> {
        run_oxc_ts(s, &Check)
    }

    #[test]
    fn flags_concurrent_without_destructured_expect() {
        assert_eq!(
            run("test.concurrent('adds', () => { expect(1).toBe(1); });").len(),
            1
        );
    }

    #[test]
    fn flags_concurrent_with_untouched_context_param() {
        assert_eq!(
            run("test.concurrent('adds', (ctx) => { expect(1).toBe(1); });").len(),
            1
        );
    }

    #[test]
    fn allows_concurrent_with_destructured_expect() {
        assert!(
            run("test.concurrent('adds', ({ expect }) => { expect(1).toBe(1); });").is_empty()
        );
    }

    #[test]
    fn allows_plain_test() {
        assert!(run("test('adds', () => { expect(1).toBe(1); });").is_empty());
    }

    #[test]
    fn flags_it_concurrent_without_destructuring() {
        assert_eq!(
            run("it.concurrent('works', async () => { expect(2).toBe(2); });").len(),
            1
        );
    }

    #[test]
    fn no_fp_when_callback_delegates_to_external_fn() {
        // Regression test for https://github.com/...#517:
        // tx-test pattern — concurrent body has no direct expect() calls.
        let src = r#"
export function txTest(handle) {
  return (name, fn) => {
    it.concurrent(name, async () => {
      const reserved = await handle.rawPg.reserve();
      try {
        await handle.txConn.run(reserved, fn);
      } finally {
        reserved.release();
      }
    });
  };
}
"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn no_fp_when_callback_body_is_empty() {
        assert!(run("it.concurrent('noop', async () => {});").is_empty());
    }

    #[test]
    fn flags_when_callback_has_expect_in_try_block() {
        assert_eq!(
            run("it.concurrent('t', async () => { try { expect(1).toBe(1); } finally {} });")
                .len(),
            1
        );
    }
}
