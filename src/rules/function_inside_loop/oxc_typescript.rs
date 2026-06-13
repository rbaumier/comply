//! function-inside-loop oxc backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

const TEST_REGISTRARS: &[&str] = &["test", "it", "describe", "bench"];
const TEST_FILE_MARKERS: &[&str] = &[".test.", ".spec.", "__tests__", "_test."];

/// Higher-order utilities that synchronously invoke their callback argument and
/// return its result. A callback passed to one of these is called now and never
/// stored, so it cannot capture a stale loop variable — flagging it is a false
/// positive. Curated allow-list; extend with additional synchronous invokers.
const SYNC_INVOKERS: &[&str] = &["untracked", "batch", "runInAction", "computed"];

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

/// Returns true when `callee` resolves to a known synchronous-invoker utility
/// (bare ident `untracked(...)` or static member `mobx.runInAction(...)`, matched
/// on the property name).
fn callee_is_sync_invoker(callee: &Expression<'_>) -> bool {
    match callee {
        Expression::Identifier(ident) => SYNC_INVOKERS.contains(&ident.name.as_str()),
        Expression::StaticMemberExpression(member) => {
            SYNC_INVOKERS.contains(&member.property.name.as_str())
        }
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

        // Callback passed to a synchronous-invoker utility (e.g. `untracked(() => ...)`):
        // it is called now and never stored, so no stale-loop-variable hazard.
        // Applies in production code too, not just test files.
        let parent = semantic.nodes().parent_node(node.id());
        if let AstKind::CallExpression(call) = parent.kind()
            && callee_is_sync_invoker(&call.callee)
        {
            let node_span = node.kind().span();
            if call.arguments.iter().any(|arg| arg.span() == node_span) {
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

    fn run(source: &str, path: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, path)
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

    #[test]
    fn allows_untracked_callback_in_for_of_prod_code() {
        let src = r#"
            for (const entry of entries) {
                const collision = untracked(() => detect(entry));
            }
        "#;
        let d = run(src, "src/app.ts");
        assert!(d.is_empty(), "expected no diagnostics, got {d:?}");
    }

    #[test]
    fn allows_batch_run_in_action_computed_callbacks() {
        for callee in ["batch", "runInAction", "computed"] {
            let src = format!(
                "for (const x of xs) {{ const r = {callee}(() => use(x)); }}"
            );
            let d = run(&src, "src/app.ts");
            assert!(d.is_empty(), "{callee}: expected no diagnostics, got {d:?}");
        }
    }

    #[test]
    fn allows_static_member_sync_invoker_callback() {
        let src = r#"
            for (const x of xs) {
                const r = mobx.runInAction(() => use(x));
            }
        "#;
        let d = run(src, "src/app.ts");
        assert!(d.is_empty(), "expected no diagnostics, got {d:?}");
    }

    #[test]
    fn flags_stored_closure_pushed_in_loop() {
        let src = r#"
            for (let i = 0; i < 10; i++) {
                arr.push(() => i);
            }
        "#;
        let d = run(src, "src/app.ts");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_unknown_single_arg_callee_in_loop() {
        let src = r#"
            for (let i = 0; i < 10; i++) {
                register(() => i);
            }
        "#;
        let d = run(src, "src/app.ts");
        assert_eq!(d.len(), 1);
    }
}
