use crate::diagnostic::Diagnostic;
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use crate::rules::test_guard_helpers::{is_narrowing_guard, test_scope_statements};
use oxc_ast::ast::{Argument, Expression, Statement};
use oxc_semantic::{NodeId, Semantic};
use std::sync::Arc;

/// The statements a guard lookup may consult, materialised on the first `if`
/// this test body contains — most test bodies have none, and the walk spans the
/// whole enclosing `describe`.
struct GuardScope<'a> {
    from: NodeId,
    semantic: &'a Semantic<'a>,
    statements: Option<Vec<&'a Statement<'a>>>,
}

impl<'a> GuardScope<'a> {
    fn new(from: NodeId, semantic: &'a Semantic<'a>) -> Self {
        Self { from, semantic, statements: None }
    }

    fn statements(&mut self) -> &[&'a Statement<'a>] {
        let (from, semantic) = (self.from, self.semantic);
        self.statements
            .get_or_insert_with(|| test_scope_statements(from, semantic))
    }
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
        semantic: &'a oxc_semantic::Semantic<'a>,
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
        let mut scope = GuardScope::new(node.id(), semantic);
        collect_control_flow(body_stmts, ctx.source, &mut scope, &mut hits);

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
    stmts: &'a [Statement<'a>],
    source: &str,
    scope: &mut GuardScope<'a>,
    out: &mut Vec<(&'static str, u32)>,
) {
    for stmt in stmts {
        collect_control_flow_stmt(stmt, source, scope, out);
    }
}

fn collect_control_flow_stmt<'a>(
    stmt: &'a Statement<'a>,
    source: &str,
    scope: &mut GuardScope<'a>,
    out: &mut Vec<(&'static str, u32)>,
) {
    match stmt {
        Statement::IfStatement(s) => {
            if !is_narrowing_guard(s, scope.statements(), source) {
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
            // Table-driven pattern: for-of whose body contains no nested
            // control flow is iterating over a fixed set of test cases to
            // register tests or run assertions — each iteration follows the
            // same linear path, so no assertion is hidden. Only flag for-of
            // loops whose body itself has if/for/while/switch inside, since
            // those can silently skip assertions on some iterations.
            let mut body_hits: Vec<(&str, u32)> = Vec::new();
            collect_control_flow_stmt(&s.body, source, scope, &mut body_hits);
            if !body_hits.is_empty() {
                out.push(("for", s.span.start));
            }
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
            collect_control_flow(&block.body, source, scope, out);
        }
        Statement::LabeledStatement(labeled) => {
            collect_control_flow_stmt(&labeled.body, source, scope, out);
        }
        Statement::TryStatement(try_stmt) => {
            collect_control_flow(&try_stmt.block.body, source, scope, out);
            if let Some(handler) = &try_stmt.handler {
                collect_control_flow(&handler.body.body, source, scope, out);
            }
            if let Some(finalizer) = &try_stmt.finalizer {
                collect_control_flow(&finalizer.body, source, scope, out);
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

    fn run_test_file(path: &str, source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, path)
    }

    #[test]
    fn flags_if_in_test() {
        let source = "test('x', () => {\n    if (true) {\n        expect(1).toBe(1);\n    }\n});";
        let diags = run_test_file("app/__tests__/foo.test.ts", source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("if"));
    }

    #[test]
    fn flags_for_in_test_with_nested_condition() {
        // for-of with an if inside can hide assertions — flag the for.
        let source = "it('x', () => {\n    for (const x of items) {\n        if (x.active) { expect(x).toBeDefined(); }\n    }\n});";
        let diags = run_test_file("src/utils.spec.ts", source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("for"));
    }

    #[test]
    fn does_not_flag_flat_for_of_in_test() {
        // for-of with no nested control flow is the table-driven pattern —
        // each iteration runs the same linear assertion path. Don't flag.
        let source = "it('checks all cases', () => {\n    for (const x of CASES) {\n        expect(fn(x)).toBe(true);\n    }\n});";
        let diags = run_test_file("src/utils.spec.ts", source);
        assert!(diags.is_empty(), "expected no diagnostics, got {diags:?}");
    }

    #[test]
    fn does_not_flag_outer_for_of_with_it_registration() {
        // Outer for-of that registers it() calls — test-registration pattern,
        // not logic inside a test body. The outer for is not inside any test.
        let source = "for (const { input, expected } of CASES) {\n    it(`${input} -> ${expected}`, () => {\n        expect(fn(input)).toBe(expected);\n    });\n}";
        let diags = run_test_file("src/utils.spec.ts", source);
        assert!(diags.is_empty(), "expected no diagnostics, got {diags:?}");
    }

    #[test]
    fn flags_traditional_for_in_test() {
        // Traditional for loop — always flag.
        let source = "it('x', () => {\n    for (let i = 0; i < 3; i++) {\n        expect(i).toBeLessThan(3);\n    }\n});";
        let diags = run_test_file("src/utils.spec.ts", source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("for"));
    }

    #[test]
    fn allows_asserted_discriminant_narrowing_issue_1056() {
        // Regression for issue #1056: discriminated-union narrowing (`if (!r.success)`)
        // preceded by an assertion on the same discriminant is type-system boilerplate.
        let source = "test('x', () => {\n    const r = z.string().min(5).safeParse('abc');\n    expect(r.success).toBe(false);\n    if (!r.success) {\n        expect(r.error.issues[0].message).toBe('m');\n    }\n});";
        let diags = run_test_file("src/locales/fr.test.ts", source);
        assert!(diags.is_empty(), "expected no diagnostics, got {diags:?}");
    }

    #[test]
    fn allows_positive_discriminant_narrowing() {
        // `if (r.success)` after `expect(r.success).toBe(true)` is the same pattern.
        let source = "test('x', () => {\n    const r = parse('a');\n    expect(r.success).toBe(true);\n    if (r.success) {\n        expect(r.data).toBe('a');\n    }\n});";
        let diags = run_test_file("src/foo.test.ts", source);
        assert!(diags.is_empty(), "expected no diagnostics, got {diags:?}");
    }

    #[test]
    fn flags_negated_discriminant_assertion_guard() {
        // `.not.toBe(true)` asserts the opposite of what the branch needs —
        // it does not make the `if` a guard.
        let source = "test('x', () => {\n    const r = parse('a');\n    expect(r.success).not.toBe(true);\n    if (r.success) {\n        expect(r.data).toBe('a');\n    }\n});";
        let diags = run_test_file("src/foo.test.ts", source);
        assert_eq!(diags.len(), 1, "expected one diagnostic, got {diags:?}");
    }

    #[test]
    fn allows_discriminant_asserted_in_a_sibling_test() {
        // Regression for #8213: the subject is bound at `describe` scope and its
        // discriminant fixed by an object-literal assertion in a sibling test —
        // the `if` only narrows the union so `dataset.value` is callable.
        let source = "describe('args', () => {\n    const dataset = run();\n    test('should return new function', () => {\n        expect(dataset).toStrictEqual({ typed: true, value: expect.any(Function) });\n    });\n    test('should not throw error for valid args', () => {\n        if (dataset.typed) {\n            expect(dataset.value(123)).toBe(123);\n        }\n    });\n});";
        let diags = run_test_file("src/args.test.ts", source);
        assert!(diags.is_empty(), "expected no diagnostics, got {diags:?}");
    }

    #[test]
    fn flags_discriminant_guard_never_asserted() {
        // Same shape with no assertion anywhere on the discriminant — genuine
        // conditional logic.
        let source = "describe('args', () => {\n    const dataset = run();\n    test('should not throw error for valid args', () => {\n        if (dataset.typed) {\n            expect(dataset.value(123)).toBe(123);\n        }\n    });\n});";
        let diags = run_test_file("src/args.test.ts", source);
        assert_eq!(diags.len(), 1, "expected one diagnostic, got {diags:?}");
    }

    #[test]
    fn flags_unasserted_member_guard_in_test() {
        // No preceding assertion on `config.enabled` — genuine conditional logic,
        // still flagged.
        let source = "test('x', () => {\n    if (config.enabled) {\n        expect(run()).toBe(1);\n    }\n});";
        let diags = run_test_file("src/foo.test.ts", source);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn ignores_non_test_file() {
        let source = "if (condition) {\n    doSomething();\n}";
        assert!(run_test_file("src/utils.ts", source).is_empty());
    }
}
