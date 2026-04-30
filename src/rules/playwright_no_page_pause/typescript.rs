//! playwright-no-page-pause — flag `page.pause()` debug-only API.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["call_expression"] prefilter = ["pause"] => |node, source, ctx, diagnostics|
    if !source.windows(16).any(|w| w == b"@playwright/test") {
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
}
