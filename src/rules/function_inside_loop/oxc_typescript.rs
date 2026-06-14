//! function-inside-loop oxc backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Expression, ForStatementLeft, VariableDeclarationKind};
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

/// True for a `for…of` / `for…in` loop whose head declares its variable with
/// `const` or `let`. Per the language spec these create a fresh, immutable (or
/// per-iteration) binding on every iteration, so a closure declared in the body
/// captures its own value — there is no shared-mutable-binding hazard, which is
/// the bug this rule targets. Excludes the C-style `for (let i…)`/`for (var i…)`
/// form (a single binding mutated across iterations) and `for (x of …)` over a
/// pre-declared target (re-assigns the same outer binding).
fn is_per_iteration_binding_loop(kind: AstKind) -> bool {
    let left = match kind {
        AstKind::ForOfStatement(stmt) => &stmt.left,
        AstKind::ForInStatement(stmt) => &stmt.left,
        _ => return false,
    };
    match left {
        ForStatementLeft::VariableDeclaration(decl) => matches!(
            decl.kind,
            VariableDeclarationKind::Const | VariableDeclarationKind::Let
        ),
        _ => false,
    }
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
            // A `for…of`/`for…in` with a `const`/`let` head binds a fresh value
            // per iteration; a closure here captures its own value safely. Keep
            // walking — an *enclosing* loop (e.g. a C-style `for` whose binding
            // this closure could still capture) must remain flagged.
            if is_per_iteration_binding_loop(kind) {
                continue;
            }
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
        // C-style loop (shared binding): the test-registrar exemption must not
        // blanket-exempt an unknown callee in a test file.
        let src = r#"
            for (let i = 0; i < cases.length; i++) {
                myCustomRegister(cases[i].label, () => {});
            }
        "#;
        let d = run(src, "src/foo.test.ts");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_vitest_pattern_outside_test_file() {
        // C-style loop (shared binding) outside a test file: the test-registrar
        // exemption is gated on test files, so this stays flagged.
        let src = r#"
            for (let i = 0; i < cases.length; i++) {
                test(cases[i].label, async () => {});
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

    #[test]
    fn allows_bench_in_for_of_const_in_bench_file() {
        // Issue #2196: Vitest Bench parameterized benchmarks registered inside a
        // `for (const n of [...])` loop. Each iteration has its own immutable
        // binding, so the arrow captures its own `n` — no closure hazard.
        let src = r#"
            describe('Exp 1: Cold mount', () => {
                for (const n of [1000, 10000, 100000, 500000]) {
                    bench(`n=${n}`, () => {
                        const v = new Virtualizer({ count: n });
                        v.calculateRange();
                    });
                }
            });
        "#;
        let d = run(src, "packages/virtual-core/tests/bench.bench.ts");
        assert!(d.is_empty(), "expected no diagnostics, got {d:?}");
    }

    #[test]
    fn allows_plain_arrow_in_for_of_const_in_prod_code() {
        // The per-iteration-binding exemption is general, not test-specific:
        // a closure inside any `for…of`/`const` loop is sound.
        let src = r#"
            for (const item of items) {
                const handler = () => process(item);
                register(handler);
            }
        "#;
        let d = run(src, "src/app.ts");
        assert!(d.is_empty(), "expected no diagnostics, got {d:?}");
    }

    #[test]
    fn allows_arrow_in_for_in_const_in_prod_code() {
        let src = r#"
            for (const key in obj) {
                const read = () => obj[key];
                register(read);
            }
        "#;
        let d = run(src, "src/app.ts");
        assert!(d.is_empty(), "expected no diagnostics, got {d:?}");
    }

    #[test]
    fn flags_arrow_in_c_style_for_let_capturing_shared_binding() {
        // Negative-space guard: the genuine bug — a closure capturing the shared
        // C-style loop binding — must STILL be flagged.
        let src = r#"
            for (let i = 0; i < n; i++) {
                setTimeout(() => console.log(i), 0);
            }
        "#;
        let d = run(src, "src/app.ts");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_arrow_in_c_style_for_var_capturing_shared_binding() {
        let src = r#"
            for (var i = 0; i < n; i++) {
                setTimeout(() => console.log(i), 0);
            }
        "#;
        let d = run(src, "src/app.ts");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_arrow_in_for_of_over_predeclared_target() {
        // `for (x of …)` reassigns the same outer binding each iteration — a
        // captured closure sees the last value, so this stays flagged.
        let src = r#"
            let x;
            for (x of items) {
                arr.push(() => x);
            }
        "#;
        let d = run(src, "src/app.ts");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_closure_in_for_of_nested_in_c_style_for() {
        // The inner loop binds per iteration, but the closure could still
        // capture the outer C-style binding `i`, so it must remain flagged.
        let src = r#"
            for (var i = 0; i < 3; i++) {
                for (const x of arr) {
                    setTimeout(() => console.log(i), 0);
                }
            }
        "#;
        let d = run(src, "src/app.ts");
        assert_eq!(d.len(), 1);
    }
}
