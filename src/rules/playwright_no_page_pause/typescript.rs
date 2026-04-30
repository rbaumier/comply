//! playwright-no-page-pause — flag `page.pause()` debug-only API.

use crate::diagnostic::{Diagnostic, Severity};

const TEST_MARKERS: &[&str] = &[".test.", ".spec.", "__tests__", "_test.", ".e2e."];

const PLAYWRIGHT_IMPORTS: &[&str] = &[
    "@playwright/test",
    "from 'playwright'",
    "from \"playwright\"",
    "require('playwright')",
    "require(\"playwright\")",
    "require('@playwright/test')",
    "require(\"@playwright/test\")",
];

fn is_test_file(path: &std::path::Path) -> bool {
    let s = path.to_string_lossy();
    TEST_MARKERS.iter().any(|m| s.contains(m))
}

fn has_playwright_import(source: &[u8]) -> bool {
    let src = std::str::from_utf8(source).unwrap_or("");
    PLAYWRIGHT_IMPORTS.iter().any(|p| src.contains(p))
}

crate::ast_check! { on ["call_expression"] prefilter = ["pause"] => |node, source, ctx, diagnostics|
    if !is_test_file(ctx.path) && !has_playwright_import(source) {
        return;
    }
    let Some(callee) = node.child_by_field_name("function") else { return };
    if callee.kind() != "member_expression" {
        return;
    }

    let Some(object) = callee.child_by_field_name("object") else { return };
    let Some(property) = callee.child_by_field_name("property") else { return };

    let obj_text = object.utf8_text(source).unwrap_or("");
    let prop_text = property.utf8_text(source).unwrap_or("");

    if prop_text == "pause" && obj_text.contains("page") {
        let pos = node.start_position();
        diagnostics.push(Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "playwright-no-page-pause".into(),
            message: "`page.pause()` is a debug-only API — remove before committing.".into(),
            severity: Severity::Error,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::backend::{AstCheck, CheckCtx};
    use std::path::Path;

    fn run(source: &str) -> Vec<Diagnostic> {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into())
            .unwrap();
        let tree = parser.parse(source, None).unwrap();
        Check.check(&CheckCtx::for_test(Path::new("test.spec.ts"), source), &tree)
    }

    fn with_import(source: &str) -> String {
        format!("import {{ test }} from \"@playwright/test\";\n{source}")
    }

    #[test]
    fn flags_page_pause() {
        let src = with_import("await page.pause();");
        let d = run(&src);
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "playwright-no-page-pause");
    }

    #[test]
    fn allows_other_pause() {
        let src = with_import("await video.pause();");
        assert!(run(&src).is_empty());
    }

    #[test]
    fn ignores_without_playwright_import() {
        let d = run("await page.pause();");
        assert!(d.is_empty());
    }

    #[test]
    fn flags_with_playwright_import() {
        let src = with_import(
            r#"test("debug pause", async ({ page }) => {
  await page.pause();
});"#,
        );
        assert_eq!(run(&src).len(), 1);
    }

    #[test]
    fn flags_via_playwright_import() {
        let src = r#"
import { test, expect } from '@playwright/test';

test('login', async ({ page }) => {
    await page.pause();
});
"#;
        let d = run("e2e/login.ts", src);
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "playwright-no-page-pause");
    }

    #[test]
    fn flags_via_playwright_require() {
        let src = r#"
const { chromium } = require('playwright');
async function run() {
    await page.pause();
}
"#;
        let d = run("tests/login.ts", src);
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn ignores_no_marker_no_import() {
        let d = run("src/utils.ts", "await page.pause();");
        assert!(d.is_empty());
    }
}
