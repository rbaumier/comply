use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

const MUTATING: &[&str] = &[
    "push",
    "pop",
    "shift",
    "unshift",
    "splice",
    "sort",
    "reverse",
    "fill",
    "copyWithin",
];

pub struct Check;

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
        let AstKind::CallExpression(call) = node.kind() else {
            return;
        };
        let Expression::StaticMemberExpression(member) = &call.callee else {
            return;
        };
        let name = member.property.name.as_str();
        if !MUTATING.contains(&name) {
            return;
        }
        // .fill() on a chained call (e.g. page.getByLabel(...).fill()) is almost
        // certainly Playwright/Locator.fill, not Array.fill.
        if name == "fill"
            && matches!(
                &member.object,
                Expression::CallExpression(_)
                    | Expression::StaticMemberExpression(_)
                    | Expression::ComputedMemberExpression(_)
            ) {
                return;
            }

        // Bounded local accumulator inside a `for` / `for-of` / `for-in`
        // loop: `const items = []; for (...) items.push(yield* fn());`.
        // The non-mutating spread alternative is O(n²) and the
        // canonical functional alternative (`Result.all(rows.map(...))`)
        // does not exist in better-result yet — tracking upstream at
        // https://github.com/dmmulroy/better-result/issues/32. Once
        // that lands, callers can switch to `Result.all` and this skip
        // becomes unnecessary.
        //
        // Same exemption inside a `Result.gen(function*() { ... })`
        // block — the generator body is the canonical accumulator site
        // for sequencing `yield*` results into a local array, and the
        // spread alternative breaks short-circuiting on the first
        // error.
        if matches!(name, "push" | "unshift")
            && matches!(&member.object, Expression::Identifier(_))
            && (is_inside_loop_body(node, semantic) || is_inside_result_gen(node, semantic))
        {
            return;
        }
        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "`.{name}()` mutates the array in place \u{2014} use a non-mutating alternative (spread, `slice`, `toSorted`, `toReversed`, `toSpliced`, `filter`, `map`, `concat`)."
            ),
            severity: Severity::Warning,
            span: None,
        });
    }
}

fn is_inside_loop_body(
    node: &oxc_semantic::AstNode,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    for ancestor in semantic.nodes().ancestors(node.id()) {
        match ancestor.kind() {
            AstKind::ForStatement(_)
            | AstKind::ForOfStatement(_)
            | AstKind::ForInStatement(_)
            | AstKind::WhileStatement(_)
            | AstKind::DoWhileStatement(_) => return true,
            // Stop at function boundary — pushes inside a callback
            // passed to a sibling helper are not "this function's
            // accumulator".
            AstKind::Function(_) | AstKind::ArrowFunctionExpression(_) => return false,
            _ => {}
        }
    }
    false
}

/// True when `node` lives inside the generator function passed to
/// `Result.gen(function*() { ... })` (or an arrow form). The generator
/// body sequences `yield*` results into a local array — that's the
/// canonical accumulator site, and the spread alternative breaks
/// short-circuiting on the first error.
fn is_inside_result_gen(
    node: &oxc_semantic::AstNode,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    let nodes = semantic.nodes();
    for ancestor in nodes.ancestors(node.id()) {
        match ancestor.kind() {
            AstKind::Function(func) if func.generator => {
                // The generator must be the direct argument of a
                // `Result.gen(...)` call.
                let parent = nodes.parent_node(ancestor.id());
                if let AstKind::CallExpression(call) = parent.kind()
                    && is_result_gen_callee(&call.callee)
                {
                    return true;
                }
                return false;
            }
            AstKind::ArrowFunctionExpression(_) => {
                let parent = nodes.parent_node(ancestor.id());
                if let AstKind::CallExpression(call) = parent.kind()
                    && is_result_gen_callee(&call.callee)
                {
                    return true;
                }
                return false;
            }
            _ => {}
        }
    }
    false
}

fn is_result_gen_callee(callee: &Expression) -> bool {
    let Expression::StaticMemberExpression(member) = callee else {
        return false;
    };
    let Expression::Identifier(obj) = &member.object else {
        return false;
    };
    matches!(obj.name.as_str(), "Result" | "Effect") && member.property.name.as_str() == "gen"
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

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, src, "t.ts")
    }

    #[test]
    fn flags_push_outside_loop() {
        let src = r#"const xs = []; xs.push(1);"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn ignores_push_inside_for_of_loop_accumulator() {
        // Regression for rbaumier/comply#36 — bounded local accumulator.
        let src = r#"
            function f(rows) {
                const items = [];
                for (const row of rows) {
                    items.push(row.id);
                }
                return items;
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_push_inside_while_loop() {
        let src = r#"
            function f() {
                const out = [];
                let i = 0;
                while (i < 10) {
                    out.push(i);
                    i++;
                }
                return out;
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn still_flags_chained_receiver_push() {
        // .foo().push() — receiver is a call, not a local identifier.
        let src = r#"function f() { for (const x of xs) state.items.push(x); }"#;
        assert!(!run(src).is_empty());
    }

    #[test]
    fn ignores_push_inside_result_gen_with_loop() {
        // Regression for rbaumier/comply#23 — canonical Result.gen accumulator.
        let src = r#"
            function mapResults(items, fn) {
                return Result.gen(function* () {
                    const mapped = [];
                    for (const item of items) {
                        mapped.push(yield* fn(item));
                    }
                    return mapped;
                });
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_push_inside_result_gen_without_loop() {
        // Regression for rbaumier/comply#23 — sequential yields inside Result.gen.
        let src = r#"
            function fetchAll() {
                return Result.gen(function* () {
                    const out = [];
                    out.push(yield* loadUser());
                    out.push(yield* loadOrders());
                    return out;
                });
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_typed_accumulator_two_step_yield_in_result_gen() {
        // Regression for rbaumier/comply#363 — exact amadeo pattern:
        // type-annotated const, two-step (separate yield + push), Result.ok wrapper.
        let src = r#"
            type User = { id: string };
            function getUsers(rows: unknown[], orgId: string) {
                return Result.gen(function* () {
                    const items: User[] = [];
                    for (const row of rows) {
                        const user = yield* rowToUser(row as any, orgId);
                        items.push(user);
                    }
                    return Result.ok({ items, total: items.length });
                });
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_push_inside_effect_gen_without_loop() {
        // Effect.gen (effect-ts) uses the same sequential-yield accumulator
        // pattern and must be treated the same as Result.gen.
        let src = r#"
            type User = { id: string };
            function fetchTwo() {
                return Effect.gen(function* () {
                    const users: User[] = [];
                    const u1 = yield* fetchUser("id1");
                    users.push(u1);
                    const u2 = yield* fetchUser("id2");
                    users.push(u2);
                    return users;
                });
            }
        "#;
        assert!(run(src).is_empty());
    }
}
