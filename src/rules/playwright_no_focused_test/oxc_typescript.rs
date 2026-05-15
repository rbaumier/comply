//! playwright-no-focused-test oxc backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

pub struct Check;

fn is_playwright_test_path(source: &str) -> bool {
    source.contains("@playwright/test") || source.contains("playwright/test")
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&[".only"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        // Limit to Playwright test files (otherwise the vitest rule
        // catches the same shape).
        if !is_playwright_test_path(ctx.source) {
            return;
        }
        let AstKind::CallExpression(call) = node.kind() else {
            return;
        };
        let Expression::StaticMemberExpression(member) = &call.callee else {
            return;
        };
        if member.property.name.as_str() != "only" {
            return;
        }
        // Accept `test.only`, `test.describe.only`.
        let receiver_ok = match &member.object {
            Expression::Identifier(id) => id.name.as_str() == "test",
            Expression::StaticMemberExpression(inner) => {
                if let Expression::Identifier(obj) = &inner.object
                    && obj.name.as_str() == "test"
                    && matches!(inner.property.name.as_str(), "describe" | "step")
                {
                    true
                } else {
                    false
                }
            }
            _ => false,
        };
        if !receiver_ok {
            return;
        }
        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Playwright `.only` skips the rest of the suite — remove before \
                      committing."
                .into(),
            severity: Severity::Error,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(src, &Check)
    }

    #[test]
    fn flags_test_only() {
        let src = r#"
            import { test } from "@playwright/test";
            test.only("x", async ({ page }) => {});
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_test_describe_only() {
        let src = r#"
            import { test } from "@playwright/test";
            test.describe.only("section", () => {});
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn ignores_when_not_a_playwright_file() {
        // No playwright import — the vitest rule handles it instead.
        let src = r#"test.only("x", () => {});"#;
        assert!(run(src).is_empty());
    }
}
