//! playwright-no-networkidle — flag `"networkidle"` wait strategy.

use crate::diagnostic::{Diagnostic, Severity};

const TEST_MARKERS: &[&str] = &[".test.", ".spec.", "__tests__", "_test.", ".e2e."];

fn is_test_file(path: &std::path::Path) -> bool {
    let s = path.to_string_lossy();
    TEST_MARKERS.iter().any(|m| s.contains(m))
}

crate::ast_check! { prefilter = ["networkidle"] => |node, source, ctx, diagnostics|
    if !is_test_file(ctx.path) {
        return;
    }
    if !source.windows(16).any(|w| w == b"@playwright/test") {
        return;
    }

    // Match string nodes whose content is "networkidle".
    if node.kind() != "string" {
        // Also check template_string.
        if node.kind() == "template_string" {
            let text = node.utf8_text(source).unwrap_or("");
            // template_string includes backticks: `networkidle`
            if text == "`networkidle`" {
                let pos = node.start_position();
                diagnostics.push(Diagnostic {
                    path: std::sync::Arc::clone(&ctx.path_arc),
                    line: pos.row + 1,
                    column: pos.column + 1,
                    rule_id: "playwright-no-networkidle".into(),
                    message: "`networkidle` is timing-based and flaky — use a web-first assertion or `waitForResponse` instead.".into(),
                    severity: Severity::Warning,
                    span: None,
                });
            }
        }
        return;
    }

    // string node includes quotes: "networkidle" or 'networkidle'
    let text = node.utf8_text(source).unwrap_or("");
    if text == "\"networkidle\"" || text == "'networkidle'" {
        let pos = node.start_position();
        diagnostics.push(Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "playwright-no-networkidle".into(),
            message: "`networkidle` is timing-based and flaky — use a web-first assertion or `waitForResponse` instead.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    use crate::rules::backend::{AstCheck, CheckCtx};

    fn run(path: &str, source: &str) -> Vec<Diagnostic> {
        let full = format!("{source}\n// @playwright/test");
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into())
            .unwrap();
        let tree = parser.parse(&full, None).unwrap();
        Check.check(&CheckCtx::for_test(Path::new(path), &full), &tree)
    }

    #[test]
    fn flags_networkidle_double_quotes() {
        let d = run(
            "nav.test.ts",
            "await page.goto('/', { waitUntil: \"networkidle\" });",
        );
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "playwright-no-networkidle");
    }

    #[test]
    fn flags_networkidle_single_quotes() {
        let d = run(
            "nav.spec.ts",
            "await page.waitForLoadState('networkidle');",
        );
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_domcontentloaded() {
        let d = run(
            "nav.test.ts",
            "await page.goto('/', { waitUntil: \"domcontentloaded\" });",
        );
        assert!(d.is_empty());
    }

    #[test]
    fn ignores_non_test_file() {
        let d = run(
            "utils.ts",
            "await page.goto('/', { waitUntil: \"networkidle\" });",
        );
        assert!(d.is_empty());
    }
}
