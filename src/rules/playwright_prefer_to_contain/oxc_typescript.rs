//! playwright-prefer-to-contain OXC backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

const TEST_MARKERS: &[&str] = &[".test.", ".spec.", "__tests__", "_test.", ".e2e."];

fn is_test_file(path: &std::path::Path) -> bool {
    let s = path.to_string_lossy();
    TEST_MARKERS.iter().any(|m| s.contains(m))
}

const EQUALITY_MATCHERS: &[&str] = &["toBe", "toEqual", "toStrictEqual"];

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
        if !crate::rules::playwright::is_playwright_context(ctx) {
            return;
        }

        let AstKind::CallExpression(call) = node.kind() else { return };

        // callee must be member: expect(...).toBe(...)
        let Expression::StaticMemberExpression(member) = &call.callee else { return };
        let matcher = member.property.name.as_str();
        if !EQUALITY_MATCHERS.contains(&matcher) {
            return;
        }

        // The arg should be true or false
        let Some(first_arg) = call.arguments.first() else { return };
        let arg_text = &ctx.source[first_arg.span().start as usize..first_arg.span().end as usize];
        if arg_text != "true" && arg_text != "false" {
            return;
        }

        // The object should be expect(…) call
        let Expression::CallExpression(expect_call) = &member.object else { return };
        let Expression::Identifier(expect_fn) = &expect_call.callee else { return };
        if expect_fn.name.as_str() != "expect" {
            return;
        }

        // The argument to expect should be a .includes() call
        let Some(first_expect_arg) = expect_call.arguments.first() else { return };
        let oxc_ast::ast::Argument::CallExpression(includes_call) = first_expect_arg else {
            return;
        };
        let Expression::StaticMemberExpression(includes_member) = &includes_call.callee else {
            return;
        };
        if includes_member.property.name.as_str() != "includes" {
            return;
        }

        let (line, column) =
            byte_offset_to_line_col(ctx.source, member.property.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Prefer using `toContain()` instead.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PW_IMPORT: &str = "import { test, expect } from \"@playwright/test\";\n";

    fn run_ts(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts_with_path(
            &format!("{PW_IMPORT}{source}"),
            &Check,
            "app.test.ts",
        )
    }

    #[test]
    fn flags_includes_to_be_true() {
        let d = run_ts("expect(arr.includes(1)).toBe(true);");
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "playwright-prefer-to-contain");
    }

    #[test]
    fn flags_includes_to_equal_false() {
        let d = run_ts("expect(arr.includes(1)).toEqual(false);");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_to_contain() {
        let d = run_ts("expect(arr).toContain(1);");
        assert!(d.is_empty());
    }
}
