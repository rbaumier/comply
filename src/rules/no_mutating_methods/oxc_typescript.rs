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
        if matches!(name, "push" | "unshift")
            && matches!(&member.object, Expression::Identifier(_))
            && is_inside_loop_body(node, semantic)
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

#[cfg(test)]
mod tests {
    use super::*;

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(src, &Check)
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
}
