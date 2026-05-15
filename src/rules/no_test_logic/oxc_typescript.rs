use crate::diagnostic::Diagnostic;
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Argument, BinaryExpression, BinaryOperator, Expression, Statement, UnaryOperator};
use std::sync::Arc;

fn is_type_narrowing(expr: &Expression) -> bool {
    match expr.without_parentheses() {
        Expression::CallExpression(call) => {
            if let Expression::StaticMemberExpression(member) = &call.callee {
                let m = member.property.name.as_str();
                return matches!(m, "isErr" | "isOk");
            }
            false
        }
        Expression::BinaryExpression(bin) => {
            matches!(bin.operator, BinaryOperator::Instanceof) || is_nullish_check(bin)
        }
        Expression::UnaryExpression(unary) => {
            matches!(unary.operator, UnaryOperator::LogicalNot)
                && is_type_narrowing(&unary.argument)
        }
        _ => false,
    }
}

fn is_nullish_check(bin: &BinaryExpression) -> bool {
    if !matches!(
        bin.operator,
        BinaryOperator::StrictInequality
            | BinaryOperator::StrictEquality
            | BinaryOperator::Inequality
            | BinaryOperator::Equality
    ) {
        return false;
    }
    is_nullish_literal(&bin.left) || is_nullish_literal(&bin.right)
}

fn is_nullish_literal(expr: &Expression) -> bool {
    matches!(expr.without_parentheses(), Expression::NullLiteral(_))
        || matches!(expr.without_parentheses(), Expression::Identifier(id) if id.name.as_str() == "undefined")
}

const TEST_MARKERS: &[&str] = &[".test.", ".spec.", "__tests__", "_test."];
const TEST_CALLEES: &[&str] = &["it", "test"];
const SETUP_HOOKS: &[&str] = &["beforeEach", "afterEach", "beforeAll", "afterAll"];

pub struct Check;

fn is_test_file(path: &std::path::Path) -> bool {
    let s = path.to_string_lossy();
    TEST_MARKERS.iter().any(|m| s.contains(m))
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
        if !is_test_file(ctx.path) {
            return;
        }
        let AstKind::CallExpression(call) = node.kind() else {
            return;
        };
        if !is_test_call(&call.callee) {
            return;
        }
        // Find the last function/arrow argument — that's the test body.
        let Some(body_stmts) = test_body_stmts(&call.arguments) else {
            return;
        };
        let mut hits: Vec<(&str, u32)> = Vec::new();
        collect_control_flow(body_stmts, ctx.source, &mut hits);

        for (label, start) in hits {
            let (line, column) = byte_offset_to_line_col(ctx.source, start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: format!(
                    "Control-flow `{label}` inside test body — tests should have a single linear assertion path."
                ),
                severity: super::META.severity,
                span: None,
            });
        }
    }
}

/// Check if the callee expression identifies a test definition.
fn is_test_call(callee: &Expression) -> bool {
    match callee {
        Expression::Identifier(id) => TEST_CALLEES.contains(&id.name.as_str()),
        Expression::StaticMemberExpression(mem) => {
            // `it.skip(...)`, `test.only(...)`, etc.
            if let Expression::Identifier(obj) = &mem.object {
                TEST_CALLEES.contains(&obj.name.as_str())
            } else {
                false
            }
        }
        Expression::CallExpression(inner) => {
            // `it.each([...])(...)` — recurse into the inner call.
            is_test_call(&inner.callee)
        }
        _ => false,
    }
}

/// Extract the statement list from the last function/arrow argument.
fn test_body_stmts<'a>(
    args: &'a oxc_allocator::Vec<'a, Argument<'a>>,
) -> Option<&'a oxc_allocator::Vec<'a, Statement<'a>>> {
    let mut last_body = None;
    for arg in args.iter() {
        let expr = arg.as_expression()?;
        match expr {
            Expression::ArrowFunctionExpression(arrow) => {
                last_body = Some(&arrow.body.statements);
            }
            Expression::FunctionExpression(func) => {
                if let Some(body) = &func.body {
                    last_body = Some(&body.statements);
                }
            }
            _ => {}
        }
    }
    last_body
}

/// Recursively find control-flow nodes in statements, skipping nested
/// function bodies and setup-hook calls.
fn collect_control_flow<'a>(
    stmts: &'a oxc_allocator::Vec<'a, Statement<'a>>,
    source: &str,
    out: &mut Vec<(&'static str, u32)>,
) {
    for stmt in stmts.iter() {
        collect_control_flow_stmt(stmt, source, out);
    }
}

fn collect_control_flow_stmt<'a>(
    stmt: &'a Statement<'a>,
    source: &str,
    out: &mut Vec<(&'static str, u32)>,
) {
    match stmt {
        Statement::IfStatement(s) => {
            if !is_type_narrowing(&s.test) {
                out.push(("if", s.span.start));
            }
        }
        Statement::ForStatement(s) => {
            out.push(("for", s.span.start));
        }
        Statement::ForInStatement(s) => {
            out.push(("for", s.span.start));
        }
        Statement::ForOfStatement(s) => {
            out.push(("for", s.span.start));
        }
        Statement::WhileStatement(s) => {
            out.push(("while", s.span.start));
        }
        Statement::DoWhileStatement(s) => {
            out.push(("while", s.span.start));
        }
        Statement::SwitchStatement(s) => {
            out.push(("switch", s.span.start));
        }
        // Skip function declarations — nested function bodies are excluded.
        Statement::FunctionDeclaration(_) => {}
        // For expression statements, check if it's a setup hook call.
        Statement::ExpressionStatement(expr_stmt) => {
            if is_setup_hook_call(&expr_stmt.expression) {
            }
            // Check for arrow/function expressions inside — skip those.
            // No control flow to find in a plain expression statement.
        }
        Statement::BlockStatement(block) => {
            collect_control_flow(&block.body, source, out);
        }
        Statement::LabeledStatement(labeled) => {
            collect_control_flow_stmt(&labeled.body, source, out);
        }
        Statement::TryStatement(try_stmt) => {
            collect_control_flow(&try_stmt.block.body, source, out);
            if let Some(handler) = &try_stmt.handler {
                collect_control_flow(&handler.body.body, source, out);
            }
            if let Some(finalizer) = &try_stmt.finalizer {
                collect_control_flow(&finalizer.body, source, out);
            }
        }
        _ => {}
    }
}

fn is_setup_hook_call(expr: &Expression) -> bool {
    if let Expression::CallExpression(call) = expr
        && let Expression::Identifier(id) = &call.callee {
            return SETUP_HOOKS.contains(&id.name.as_str());
        }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_test_file(path: &str, source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts_with_path(source, &Check, path)
    }

    #[test]
    fn flags_if_in_test() {
        let source = "test('x', () => {\n    if (true) {\n        expect(1).toBe(1);\n    }\n});";
        let diags = run_test_file("app/__tests__/foo.test.ts", source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("if"));
    }

    #[test]
    fn flags_for_in_test() {
        let source = "it('does stuff', () => {\n    for (const x of items) {\n        expect(x).toBeDefined();\n    }\n});";
        let diags = run_test_file("src/utils.spec.ts", source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("for"));
    }

    #[test]
    fn ignores_non_test_file() {
        let source = "if (condition) {\n    doSomething();\n}";
        assert!(run_test_file("src/utils.ts", source).is_empty());
    }
}
