//! no-conditional-tests oxc backend — flag `describe`/`test`/`it` calls wrapped
//! in conditional control flow (if, ternary, switch case).

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

const TEST_FNS: &[&str] = &["describe", "test", "it"];

pub struct Check;

fn callee_base_name<'a>(callee: &'a Expression<'a>) -> Option<&'a str> {
    match callee {
        Expression::Identifier(id) => Some(id.name.as_str()),
        Expression::StaticMemberExpression(member) => {
            // e.g. `test.each`, `describe.only` — extract the object identifier
            if let Expression::Identifier(obj) = &member.object {
                Some(obj.name.as_str())
            } else {
                None
            }
        }
        Expression::CallExpression(inner_call) => {
            // e.g. `test.each([1])('a', ...)` — the outer callee is a call
            callee_base_name(&inner_call.callee)
        }
        _ => None,
    }
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
        let AstKind::CallExpression(call) = node.kind() else { return };

        let Some(base) = callee_base_name(&call.callee) else { return };
        if !TEST_FNS.contains(&base) {
            return;
        }

        // Walk ancestors looking for conditional control flow.
        // Two-state walk: if a conditional is found inside a loop boundary,
        // it is a parametric pattern (e.g. dialect filter) — skip the diagnostic.
        let mut found_conditional = false;
        for ancestor_kind in semantic.nodes().ancestor_kinds(node.id()) {
            match ancestor_kind {
                AstKind::IfStatement(_)
                | AstKind::ConditionalExpression(_)
                | AstKind::SwitchStatement(_) => {
                    found_conditional = true;
                }
                AstKind::ForStatement(_)
                | AstKind::ForOfStatement(_)
                | AstKind::ForInStatement(_)
                | AstKind::WhileStatement(_)
                | AstKind::DoWhileStatement(_) => {
                    if found_conditional {
                        // Conditional is nested inside a loop — parametric pattern, not a FP.
                        return;
                    }
                }
                AstKind::Function(_) | AstKind::ArrowFunctionExpression(_) => {
                    // Stop at function boundary to avoid false exemptions from outer loops.
                    break;
                }
                _ => {}
            }
        }
        if found_conditional {
            let (line, column) =
                byte_offset_to_line_col(ctx.source, call.span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: "Don't conditionally define tests, use test.skip or describe.skip"
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

    fn run_on(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(src, &Check)
    }

    #[test]
    fn flags_test_inside_if() {
        let src = "if (flag) { test('a', () => {}); }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_describe_inside_ternary() {
        let src = "flag ? describe('a', () => {}) : null;";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_it_inside_switch() {
        let src = "switch (x) { case 1: it('a', () => {}); break; }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_top_level_test() {
        let src = "test('a', () => {});";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn no_fp_on_dialect_filter_in_for_of_loop() {
        // Regression for #826 — parametric dialect filter in multi-backend suite.
        let src = r#"
            const DIALECTS = ['mysql', 'sqlite', 'postgres'];
            for (const dialect of DIALECTS) {
                if (dialect === 'mysql' || dialect === 'sqlite') {
                    describe(`${dialect}: replace into`, () => {
                        test('works', () => {});
                    });
                }
            }
        "#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn no_fp_on_switch_in_for_of_loop() {
        // Regression for #826 — switch on loop variable is also parametric.
        let src = r#"
            const DIALECTS = ['mysql', 'sqlite', 'postgres'];
            for (const dialect of DIALECTS) {
                switch (dialect) {
                    case 'mysql':
                        describe(`${dialect}: mysql only`, () => {
                            test('works', () => {});
                        });
                        break;
                }
            }
        "#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_if_in_arrow_fn_wrapping_loop() {
        // Arrow function boundary prevents exemption from outer loop.
        let src = r#"
            for (const x of xs) {
                (() => {
                    if (cond) {
                        test('a', () => {});
                    }
                })();
            }
        "#;
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_if_wrapping_for_loop() {
        // if is the outer ancestor, loop is inside — still conditional.
        let src = r#"
            if (cond) {
                for (const x of xs) {
                    test(x, () => {});
                }
            }
        "#;
        assert_eq!(run_on(src).len(), 1);
    }
}
