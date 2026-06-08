use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

const TEST_MARKERS: &[&str] = &[".test.", ".spec.", "__tests__", "_test.", ".e2e."];

fn is_test_file(path: &std::path::Path) -> bool {
    let s = path.to_string_lossy();
    TEST_MARKERS.iter().any(|m| s.contains(m))
}

const LOCATOR_METHODS: &[&str] = &[
    "isVisible",
    "isHidden",
    "isEnabled",
    "isDisabled",
    "isChecked",
    "isEditable",
    "textContent",
    "innerText",
    "innerHTML",
    "getAttribute",
    "inputValue",
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

        // Check if this is `expect(...)`.
        let Expression::Identifier(callee) = &call.callee else { return };
        if callee.name.as_str() != "expect" {
            return;
        }

        // First argument should be an await expression.
        let Some(first_arg) = call.arguments.first() else { return };
        let Some(arg_expr) = first_arg.as_expression() else { return };
        let Expression::AwaitExpression(await_expr) = arg_expr else { return };

        // The awaited expression should be a call with a locator method.
        let Expression::CallExpression(inner_call) = &await_expr.argument else { return };
        let Expression::StaticMemberExpression(member) = &inner_call.callee else { return };
        let method_name = member.property.name.as_str();
        if !LOCATOR_METHODS.contains(&method_name) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Use web-first assertions (`toBeVisible`, \
                      `toBeEnabled`, etc.) instead of asserting on \
                      awaited locator methods — they auto-retry."
                .into(),
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
        crate::rules::test_helpers::run_oxc_ts_with_path(&full, &Check, "login.test.ts")
    }


    #[test]
    fn flags_is_visible_assertion() {
        let d = run_on("expect(await page.locator('#btn').isVisible()).toBe(true);");
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "playwright-prefer-web-first-assertions");
    }


    #[test]
    fn flags_text_content_assertion() {
        let d = run_on("expect(await el.textContent()).toContain('Hello');");
        assert_eq!(d.len(), 1);
    }


    #[test]
    fn allows_web_first_assertion() {
        let d = run_on("await expect(page.locator('#btn')).toBeVisible();");
        assert!(d.is_empty());
    }


    #[test]
    fn allows_expect_await_with_non_locator() {
        let d = run_on("expect(await fetch('/api')).toBeDefined();");
        assert!(d.is_empty());
    }


    #[test]
    fn ignores_non_test_file() {
        let d = crate::rules::test_helpers::run_oxc_ts_with_path(
            "expect(await el.isVisible()).toBe(true);",
            &Check,
            "helpers.ts",
        );
        assert!(d.is_empty());
    }
}
