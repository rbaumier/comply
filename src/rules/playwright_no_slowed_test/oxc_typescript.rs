//! playwright-no-slowed-test oxc backend — flag zero-argument `test.slow()` calls.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

pub struct Check;

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
        if !crate::rules::playwright::is_playwright_context(ctx) {
            return;
        }

        let AstKind::CallExpression(call) = node.kind() else { return };

        let Expression::StaticMemberExpression(member) = &call.callee else { return };
        if member.property.name.as_str() != "slow" {
            return;
        }
        let Expression::Identifier(obj) = &member.object else { return };
        if obj.name.as_str() != "test" {
            return;
        }

        // Only flag the unconditional (zero-argument) form.
        if !call.arguments.is_empty() {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`test.slow()` without arguments marks the test as always slow \u{2014} optimize it or use the conditional form `test.slow(condition, reason)`.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;



    fn run_on(source: &str) -> Vec<Diagnostic> {
        let full = format!("import {{ test, expect }} from \"@playwright/test\";\n{source}");
        crate::rules::test_helpers::run_oxc_ts(&full, &Check)
    }


    #[test]
    fn flags_bare_test_slow() {
        let src = "test.slow();";
        assert_eq!(run_on(src).len(), 1);
    }


    #[test]
    fn flags_test_slow_inside_test() {
        let src = "test('my test', () => { test.slow(); });";
        assert_eq!(run_on(src).len(), 1);
    }


    #[test]
    fn allows_conditional_test_slow() {
        let src = "test('my test', () => { test.slow(process.env.CI, 'CI is slow'); });";
        assert!(run_on(src).is_empty());
    }


    #[test]
    fn allows_normal_test() {
        let src = "test('my test', () => { expect(1).toBe(1); });";
        assert!(run_on(src).is_empty());
    }


    #[test]
    fn ignores_other_slow_methods() {
        let src = "foo.slow();";
        assert!(run_on(src).is_empty());
    }
}
