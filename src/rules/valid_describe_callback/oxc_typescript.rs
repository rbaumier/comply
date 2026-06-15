//! OXC backend for valid-describe-callback.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

pub struct Check;

/// Check if a call expression's callee is `describe` (bare) or
/// `describe.skip` / `describe.only` / `describe.each(...)`.
fn is_describe_callee(callee: &Expression) -> bool {
    match callee {
        Expression::Identifier(id) => id.name.as_str() == "describe",
        Expression::StaticMemberExpression(member) => {
            is_describe_callee(&member.object)
        }
        Expression::CallExpression(call) => {
            is_describe_callee(&call.callee)
        }
        _ => false,
    }
}

/// Return true when the call is a parameterized describe form —
/// `describe.each(table)(name, fn)` or `describe.for(table)(name, fn)` (and
/// chained variants like `describe.concurrent.for(...)`). The `fn` callback
/// receives the table row as arguments, so parameters are expected and must
/// not be flagged.
fn callee_is_parameterized_describe(callee: &Expression) -> bool {
    let Expression::CallExpression(inner) = callee else {
        return false;
    };
    let Expression::StaticMemberExpression(member) = &inner.callee else {
        return false;
    };
    let prop = member.property.name.as_str();
    (prop == "each" || prop == "for") && is_describe_callee(&member.object)
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

        if !is_describe_callee(&call.callee) {
            return;
        }

        // The callback is the second argument
        if call.arguments.len() < 2 {
            return;
        }
        let cb = &call.arguments[1];

        let is_parameterized = callee_is_parameterized_describe(&call.callee);

        match cb {
            oxc_ast::ast::Argument::ArrowFunctionExpression(arrow) => {
                let is_async = arrow.r#async;
                let has_params = !is_parameterized && !arrow.params.items.is_empty();
                let returns_value = if arrow.expression {
                    // Arrow with expression body = implicit return. A bare call
                    // (`() => helper(arg)`) invokes a side-effecting suite helper
                    // that registers nested `it`/`describe` blocks; the implicit
                    // return is its (void) result, not a meaningful value.
                    !expression_body_is_bare_call(&arrow.body.statements)
                } else {
                    body_returns_value_stmts(&arrow.body.statements)
                };

                let message = if is_async {
                    "`describe` callback must not be async."
                } else if has_params {
                    "`describe` callback must not declare parameters."
                } else if returns_value {
                    "`describe` callback must not return a value."
                } else {
                    return;
                };

                let (line, column) = byte_offset_to_line_col(ctx.source, arrow.span.start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: "valid-describe-callback".into(),
                    message: message.into(),
                    severity: Severity::Warning,
                    span: Some((arrow.span.start as usize, (arrow.span.end - arrow.span.start) as usize)),
                });
            }
            oxc_ast::ast::Argument::FunctionExpression(func) => {
                let is_async = func.r#async;
                let has_params = !is_parameterized && !func.params.items.is_empty();
                let returns_value = func.body.as_ref()
                    .map(|body| body_returns_value_stmts(&body.statements))
                    .unwrap_or(false);

                let message = if is_async {
                    "`describe` callback must not be async."
                } else if has_params {
                    "`describe` callback must not declare parameters."
                } else if returns_value {
                    "`describe` callback must not return a value."
                } else {
                    return;
                };

                let (line, column) = byte_offset_to_line_col(ctx.source, func.span.start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: "valid-describe-callback".into(),
                    message: message.into(),
                    severity: Severity::Warning,
                    span: Some((func.span.start as usize, (func.span.end - func.span.start) as usize)),
                });
            }
            _ => {}
        }
    }
}

/// True when an expression-body arrow (`() => expr`) has `expr` as a bare
/// call expression. oxc stores the implicit-return expression as the sole
/// `ExpressionStatement` of the arrow body.
fn expression_body_is_bare_call(stmts: &[oxc_ast::ast::Statement]) -> bool {
    use oxc_ast::ast::Statement;
    let [Statement::ExpressionStatement(expr_stmt)] = stmts else {
        return false;
    };
    matches!(expr_stmt.expression, Expression::CallExpression(_))
}

/// Walk statements looking for a `return` with a value, without descending
/// into nested functions.
fn body_returns_value_stmts(stmts: &[oxc_ast::ast::Statement]) -> bool {
    use oxc_ast::ast::Statement;
    for stmt in stmts.iter() {
        match stmt {
            Statement::ReturnStatement(ret) => {
                if ret.argument.is_some() {
                    return true;
                }
            }
            Statement::BlockStatement(block) => {
                if body_returns_value_stmts(&block.body) {
                    return true;
                }
            }
            Statement::IfStatement(if_stmt) => {
                if stmt_returns_value(&if_stmt.consequent) {
                    return true;
                }
                if let Some(ref alt) = if_stmt.alternate
                    && stmt_returns_value(alt) {
                        return true;
                    }
            }
            // Don't descend into nested function declarations/expressions
            Statement::FunctionDeclaration(_) => continue,
            Statement::ExpressionStatement(expr_stmt) => {
                // Skip function expressions / arrow functions at statement level
                match &expr_stmt.expression {
                    Expression::ArrowFunctionExpression(_) | Expression::FunctionExpression(_) => continue,
                    _ => {}
                }
            }
            _ => {}
        }
    }
    false
}

fn stmt_returns_value(stmt: &oxc_ast::ast::Statement) -> bool {
    use oxc_ast::ast::Statement;
    match stmt {
        Statement::ReturnStatement(ret) => ret.argument.is_some(),
        Statement::BlockStatement(block) => body_returns_value_stmts(&block.body),
        Statement::IfStatement(if_stmt) => {
            stmt_returns_value(&if_stmt.consequent)
                || if_stmt.alternate.as_ref().is_some_and(|alt| stmt_returns_value(alt))
        }
        Statement::FunctionDeclaration(_) => false,
        _ => false,
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
    

    // Regression #516 — describe.each callback receives row params; must not be flagged.
    #[test]
    fn allows_describe_each_with_destructured_param() {
        let d = crate::rules::test_helpers::run_rule(&Check, "const HOOKS = [{ action: 'deactivate' }]; \
             describe.each(HOOKS)('$action category', ({ action }) => { it('x', () => {}); });", "t.ts");
        assert!(d.is_empty(), "unexpected diagnostics: {d:?}");
    }

    #[test]
    fn allows_describe_each_with_multiple_params() {
        let d = crate::rules::test_helpers::run_rule(&Check, "describe.each([[1, 2]])('sum', (a, b) => { it('x', () => {}); });", "t.ts");
        assert!(d.is_empty(), "unexpected diagnostics: {d:?}");
    }

    #[test]
    fn allows_describe_each_tsx_with_typed_params() {
        let d = crate::rules::test_helpers::run_rule(&Check, "describe.each([['foo', fn1]])('%s', (_label, decision) => { it('x', () => {}); });", "t.tsx");
        assert!(d.is_empty(), "unexpected diagnostics: {d:?}");
    }

    // Regression #3341 — describe.for is the sibling parameterized API; its
    // callback receives the table row and must not be flagged for parameters.
    #[test]
    fn allows_describe_for_with_destructured_param() {
        let d = crate::rules::test_helpers::run_rule(&Check, "describe.for([[1, 1], [1, 2], [2, 1]])('add(%i, %i)', ([a, b]) => { test('test', () => { expect(a + b).matchSnapshot(); }); });", "t.ts");
        assert!(d.is_empty(), "unexpected diagnostics: {d:?}");
    }

    #[test]
    fn allows_describe_concurrent_for_with_param() {
        let d = crate::rules::test_helpers::run_rule(&Check, "describe.concurrent.for([1, 2])('concurrent %i', (item) => { test('is marked concurrent', () => { expect(item).toBeGreaterThan(0); }); });", "t.ts");
        assert!(d.is_empty(), "unexpected diagnostics: {d:?}");
    }

    #[test]
    fn still_flags_describe_for_with_async_callback() {
        let d = crate::rules::test_helpers::run_rule(&Check, "describe.for([1])('suite', async (item) => { it('x', () => {}); });", "t.ts");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("async"));
    }

    #[test]
    fn still_flags_describe_each_with_async_callback() {
        let d = crate::rules::test_helpers::run_rule(&Check, "describe.each([{}])('suite', async ({ x }) => { it('x', () => {}); });", "t.ts");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("async"));
    }

    #[test]
    fn still_flags_plain_describe_with_params() {
        let d = crate::rules::test_helpers::run_rule(&Check, "describe('suite', (done) => { it('x', () => {}); });", "t.ts");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("parameters"));
    }

    // Regression #2351 — expression-body arrow calling a void suite helper.
    #[test]
    fn allows_expression_body_arrow_calling_suite_helper() {
        let d = crate::rules::test_helpers::run_rule(&Check, "describe('when 204', () => strip(204));", "t.ts");
        assert!(d.is_empty(), "unexpected diagnostics: {d:?}");
    }

    #[test]
    fn allows_expression_body_arrow_calling_member_suite_helper() {
        let d = crate::rules::test_helpers::run_rule(&Check, "describe('when X', () => helpers.run(arg));", "t.ts");
        assert!(d.is_empty(), "unexpected diagnostics: {d:?}");
    }

    #[test]
    fn still_flags_expression_body_arrow_returning_object() {
        let d = crate::rules::test_helpers::run_rule(&Check, "describe('x', () => ({ a: 1 }));", "t.ts");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("return a value"));
    }

    #[test]
    fn still_flags_block_body_arrow_returning_value() {
        let d = crate::rules::test_helpers::run_rule(&Check, "describe('x', () => { return promise; });", "t.ts");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("return a value"));
    }

    #[test]
    fn still_flags_async_describe_callback() {
        let d = crate::rules::test_helpers::run_rule(&Check, "describe('x', async () => {});", "t.ts");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("async"));
    }
}
