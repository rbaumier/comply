use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

pub struct Check;

const TEST_MARKERS: &[&str] = &[".test.", ".spec.", "__tests__", "_test.", ".e2e."];

fn is_test_file(path: &std::path::Path) -> bool {
    let s = path.to_string_lossy();
    TEST_MARKERS.iter().any(|m| s.contains(m))
}

const MATCHER_PAIRS: &[(&str, &str)] = &[
    ("toBeVisible", "toBeHidden"),
    ("toBeHidden", "toBeVisible"),
    ("toBeEnabled", "toBeDisabled"),
    ("toBeDisabled", "toBeEnabled"),
];

fn inverse_of(matcher: &str) -> Option<&'static str> {
    MATCHER_PAIRS
        .iter()
        .find(|(m, _)| *m == matcher)
        .map(|(_, inv)| *inv)
}

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

        // Pattern: expect(x).not.toBeVisible()
        // callee = member_expression { object: member_expression { object: call(expect), property: "not" }, property: "toBeVisible" }
        let Expression::StaticMemberExpression(outer) = &call.callee else { return };
        let matcher_name = outer.property.name.as_str();

        let Some(inverse) = inverse_of(matcher_name) else { return };

        let Expression::StaticMemberExpression(inner) = &outer.object else { return };
        if inner.property.name != "not" {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: "playwright-no-useless-not".into(),
            message: format!("Unexpected usage of not.{matcher_name}(). Use {inverse}() instead."),
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
    fn flags_not_to_be_visible() {
        let d = run_oxc_ts("await expect(el).not.toBeVisible();");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("toBeHidden"));
    }


    #[test]
    fn flags_not_to_be_enabled() {
        let d = run_oxc_ts("await expect(el).not.toBeEnabled();");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("toBeDisabled"));
    }


    #[test]
    fn allows_not_to_be() {
        let d = run_oxc_ts("await expect(el).not.toBe(1);");
        assert!(d.is_empty());
    }
}
