//! OxcCheck backend for no-small-switch — flag `switch` with fewer than N `case` clauses.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

/// An exhaustiveness `default` arm (`const _x: never = …`, `… as never`, or an
/// `assertNever`/`assertUnreachable` call) turns the switch into a compile-time
/// guard against unhandled variants — adding a discriminant variant then fails
/// to compile. The small case count is intentional, not a smell.
fn has_exhaustive_default(switch: &oxc_ast::ast::SwitchStatement, source: &str) -> bool {
    switch.cases.iter().filter(|c| c.test.is_none()).any(|c| {
        let text = &source[c.span.start as usize..c.span.end as usize];
        text.contains(": never")
            || text.contains(":never")
            || text.contains("as never")
            || text.contains("assertNever")
            || text.contains("assertUnreachable")
    })
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::SwitchStatement]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::SwitchStatement(switch) = node.kind() else { return };
        // Count only non-default cases (cases with a test expression).
        let case_count = switch.cases.iter().filter(|c| c.test.is_some()).count();
        let min_cases = ctx.config.threshold("no-small-switch", "min_cases", ctx.lang);
        if case_count < min_cases && !has_exhaustive_default(switch, ctx.source) {
            let (line, column) = byte_offset_to_line_col(ctx.source, switch.span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: format!("`switch` has only {case_count} case(s) — use `if/else` instead."),
                severity: Severity::Warning,
                span: None,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
    }

    #[test]
    fn flags_switch_with_two_cases() {
        let src = "switch (x) { case 1: break; case 2: break; }";
        assert_eq!(run_on(src).len(), 1);
    }

    // Regression for #256: a two-variant discriminated dispatch with a
    // `never`-asserting default arm is an exhaustiveness guard, not a smell.
    #[test]
    fn allows_two_case_switch_with_never_default() {
        let src = r#"
            function pick(intent: Intent): string | null {
                switch (intent.kind) {
                    case "create": return intent.scope;
                    case "edit": return intent.newScope;
                    default: {
                        const _exhaustive: never = intent;
                        return _exhaustive;
                    }
                }
            }
        "#;
        assert!(run_on(src).is_empty(), "{:?}", run_on(src));
    }

    #[test]
    fn allows_two_case_switch_with_assert_never_default() {
        let src = r#"
            switch (x.kind) {
                case "a": return 1;
                case "b": return 2;
                default: return assertNever(x);
            }
        "#;
        assert!(run_on(src).is_empty(), "{:?}", run_on(src));
    }
}
