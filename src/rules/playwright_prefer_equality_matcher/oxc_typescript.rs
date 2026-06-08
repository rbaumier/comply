//! playwright-prefer-equality-matcher OXC backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Argument, Expression};
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

        // The matcher arg must be true/false
        let Some(first_arg) = call.arguments.first() else { return };
        let arg_text = &ctx.source[first_arg.span().start as usize..first_arg.span().end as usize];
        if arg_text != "true" && arg_text != "false" {
            return;
        }

        // The object should be expect(binary_expression) with === or !==
        let Expression::CallExpression(expect_call) = &member.object else { return };
        let Expression::Identifier(expect_fn) = &expect_call.callee else { return };
        if expect_fn.name.as_str() != "expect" {
            return;
        }

        let Some(first_expect_arg) = expect_call.arguments.first() else { return };
        let Argument::BinaryExpression(bin) = first_expect_arg else { return };

        use oxc_ast::ast::BinaryOperator;
        if !matches!(
            bin.operator,
            BinaryOperator::StrictEquality | BinaryOperator::StrictInequality
        ) {
            return;
        }

        let (line, column) =
            byte_offset_to_line_col(ctx.source, member.property.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Prefer using one of the equality matchers instead.".into(),
            severity: Severity::Warning,
            span: None,
        });
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

    const PW_IMPORT: &str = "import { test, expect } from \"@playwright/test\";\n";

    fn run_ts(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, &format!("{PW_IMPORT}{source}"), "app.test.ts")
    }

    #[test]
    fn flags_strict_equality() {
        let d = run_ts("expect(a === b).toBe(true);");
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "playwright-prefer-equality-matcher");
    }

    #[test]
    fn flags_strict_inequality() {
        let d = run_ts("expect(a !== b).toEqual(true);");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_direct_equality() {
        let d = run_ts("expect(a).toBe(b);");
        assert!(d.is_empty());
    }
}
