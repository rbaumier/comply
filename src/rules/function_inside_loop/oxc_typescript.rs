//! function-inside-loop oxc backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

pub struct Check;

const TEST_REGISTRARS: &[&str] = &["test", "it", "describe", "bench"];
const TEST_FILE_MARKERS: &[&str] = &[".test.", ".spec.", "__tests__", "_test."];

fn is_test_file(path: &std::path::Path) -> bool {
    let s = path.to_string_lossy();
    TEST_FILE_MARKERS.iter().any(|m| s.contains(m))
}

fn is_loop(kind: AstKind) -> bool {
    matches!(
        kind,
        AstKind::ForStatement(_)
            | AstKind::ForInStatement(_)
            | AstKind::ForOfStatement(_)
            | AstKind::WhileStatement(_)
            | AstKind::DoWhileStatement(_)
    )
}

fn is_function_boundary(kind: AstKind) -> bool {
    matches!(
        kind,
        AstKind::Function(_)
            | AstKind::ArrowFunctionExpression(_)
            | AstKind::MethodDefinition(_)
    )
}

/// Returns true when `callee` resolves to a vitest test-registrar (bare ident, static member, or chained `each(...)` form).
fn callee_is_test_registrar(callee: &Expression<'_>) -> bool {
    match callee {
        Expression::Identifier(ident) => TEST_REGISTRARS.contains(&ident.name.as_str()),
        Expression::StaticMemberExpression(member) => {
            if let Expression::Identifier(obj) = &member.object {
                TEST_REGISTRARS.contains(&obj.name.as_str())
            } else {
                false
            }
        }
        Expression::CallExpression(inner) => callee_is_test_registrar(&inner.callee),
        _ => false,
    }
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[
            AstType::Function,
            AstType::ArrowFunctionExpression,
        ]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        // Dynamic test registration — loop is intentional.
        if is_test_file(ctx.path) {
            let parent = semantic.nodes().parent_node(node.id());
            if let AstKind::CallExpression(call) = parent.kind()
                && callee_is_test_registrar(&call.callee)
            {
                return;
            }
        }

        // Walk ancestors looking for a loop. Stop at function boundaries.
        let mut first = true;
        for ancestor in semantic.nodes().ancestors(node.id()) {
            // Skip self.
            if first {
                first = false;
                continue;
            }
            let kind = ancestor.kind();
            if is_loop(kind) {
                let span = match node.kind() {
                    AstKind::Function(f) => f.span,
                    AstKind::ArrowFunctionExpression(a) => a.span,
                    _ => return,
                };
                let (line, column) =
                    byte_offset_to_line_col(ctx.source, span.start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: "Function declared inside loop \u{2014} creates new function object each iteration.".into(),
                    severity: Severity::Warning,
                    span: None,
                });
                return;
            }
            // Stop at enclosing function boundaries (not counting self).
            if is_function_boundary(kind) {
                return;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(source: &str, path: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts_with_path(source, &Check, path)
    }

    #[test]
    fn flags_arrow_in_for_in_prod_code() {
        let d = run(
            "for (let i = 0; i < 10; i++) { const fn = () => i; }",
            "src/app.ts",
        );
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_function_in_while_in_prod_code() {
        let d = run(
            "while (true) { const fn = function() {}; }",
            "src/app.ts",
        );
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_vitest_test_inside_for_of_loop_in_test_file() {
        let src = r#"
            import { test } from "vitest";

            const cases = [
                { label: "case A", build: () => ({ x: 1 }) },
                { label: "case B", build: () => ({ x: 2 }) },
            ];

            for (const { label, build } of cases) {
                test(label, async () => {
                    const fixture = build();
                    expect(fixture.x).toBeGreaterThan(0);
                });
            }
        "#;
        let d = run(
            src,
            "src/api/features/teams/edit-team.integration.test.ts",
        );
        assert!(d.is_empty(), "expected no diagnostics, got {d:?}");
    }

    #[test]
    fn allows_vitest_test_dynamic_registration() {
        let src = r#"
            import { test } from "vitest";
            const cases = [{ label: "a" }, { label: "b" }];
            for (const { label } of cases) {
                test(label, async () => {});
            }
        "#;
        let d = run(src, "src/foo.test.ts");
        assert!(d.is_empty(), "expected no diagnostics, got {d:?}");
    }

    #[test]
    fn allows_vitest_it_concurrent_dynamic_registration() {
        let src = r#"
            for (const c of cases) {
                it.concurrent(c.label, () => {});
            }
        "#;
        let d = run(src, "src/foo.test.ts");
        assert!(d.is_empty(), "expected no diagnostics, got {d:?}");
    }

    #[test]
    fn allows_vitest_test_each_table_form() {
        let src = r#"
            for (const c of cases) {
                test.each([1, 2])(c.label, (row) => {});
            }
        "#;
        let d = run(src, "src/foo.spec.ts");
        assert!(d.is_empty(), "expected no diagnostics, got {d:?}");
    }

    #[test]
    fn allows_describe_skip_dynamic_registration() {
        let src = r#"
            for (const c of cases) {
                describe.skip(c.label, () => {});
            }
        "#;
        let d = run(src, "src/foo.test.ts");
        assert!(d.is_empty(), "expected no diagnostics, got {d:?}");
    }

    #[test]
    fn flags_unknown_callee_even_in_test_file() {
        let src = r#"
            for (const c of cases) {
                myCustomRegister(c.label, () => {});
            }
        "#;
        let d = run(src, "src/foo.test.ts");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_vitest_pattern_outside_test_file() {
        let src = r#"
            for (const c of cases) {
                test(c.label, async () => {});
            }
        "#;
        let d = run(src, "src/app.ts");
        assert_eq!(d.len(), 1);
    }
}
