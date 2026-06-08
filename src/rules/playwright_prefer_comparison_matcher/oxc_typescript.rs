//! playwright-prefer-comparison-matcher oxc backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Argument, BinaryOperator, Expression};
use std::sync::Arc;

const TEST_MARKERS: &[&str] = &[".test.", ".spec.", "__tests__", "_test.", ".e2e."];

fn is_test_file(path: &std::path::Path) -> bool {
    let s = path.to_string_lossy();
    TEST_MARKERS.iter().any(|m| s.contains(m))
}

const EQUALITY_MATCHERS: &[&str] = &["toBe", "toEqual", "toStrictEqual"];

fn preferred_matcher(op: BinaryOperator) -> Option<&'static str> {
    match op {
        BinaryOperator::GreaterThan => Some("toBeGreaterThan"),
        BinaryOperator::GreaterEqualThan => Some("toBeGreaterThanOrEqual"),
        BinaryOperator::LessThan => Some("toBeLessThan"),
        BinaryOperator::LessEqualThan => Some("toBeLessThanOrEqual"),
        _ => None,
    }
}

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
        if !is_test_file(ctx.path) {
            return;
        }
        if !crate::rules::playwright::is_playwright_context(ctx) {
            return;
        }

        let AstKind::CallExpression(call) = node.kind() else {
            return;
        };

        // Callee must be `<something>.<matcher>`
        let Expression::StaticMemberExpression(member) = &call.callee else {
            return;
        };
        let matcher = member.property.name.as_str();
        if !EQUALITY_MATCHERS.contains(&matcher) {
            return;
        }

        // Argument must be `true` or `false`
        let Some(first_arg) = call.arguments.first() else {
            return;
        };
        let Argument::BooleanLiteral(_) = first_arg else {
            return;
        };

        // The object should be `expect(binary_expression)`
        let Expression::CallExpression(expect_call) = &member.object else {
            return;
        };
        let Expression::Identifier(expect_fn) = &expect_call.callee else {
            return;
        };
        if expect_fn.name.as_str() != "expect" {
            return;
        }

        let Some(first_expect_arg) = expect_call.arguments.first() else {
            return;
        };
        let Argument::BinaryExpression(bin) = first_expect_arg else {
            return;
        };

        let Some(preferred) = preferred_matcher(bin.operator) else {
            return;
        };

        let (line, column) =
            byte_offset_to_line_col(ctx.source, member.property.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!("Prefer using `{preferred}` instead."),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::test_helpers::run_oxc_ts_with_path;

    const PW_IMPORT: &str = "import { test, expect } from \"@playwright/test\";\n";


    fn run_oxc_ts(source: &str) -> Vec<Diagnostic> {
        run_oxc_ts_with_path(&format!("{PW_IMPORT}{source}"), &Check, "app.test.ts")
    }


    #[test]
    fn flags_greater_than_comparison() {
        let d = run_oxc_ts("expect(a > b).toBe(true);");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("toBeGreaterThan"));
    }


    #[test]
    fn flags_less_than_or_equal() {
        let d = run_oxc_ts("expect(a <= b).toEqual(true);");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("toBeLessThanOrEqual"));
    }


    #[test]
    fn allows_non_comparison() {
        let d = run_oxc_ts("expect(a).toBe(true);");
        assert!(d.is_empty());
    }
}
