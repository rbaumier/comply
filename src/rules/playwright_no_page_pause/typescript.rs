//! playwright-no-page-pause — flag `page.pause()` debug-only API.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["call_expression"] prefilter = ["pause"] => |node, source, ctx, diagnostics|
    if !crate::rules::playwright::is_playwright_context(ctx) {
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

    fn run(path: &str, source: &str) -> Vec<Diagnostic> {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into())
            .unwrap();
        let tree = parser.parse(source, None).unwrap();
        Check.check(&CheckCtx::for_test(Path::new(path), source), &tree)
    }

    fn with_import(source: &str) -> String {
        format!("import {{ test }} from \"@playwright/test\";\n{source}")
    }

    #[test]
    fn flags_page_pause_with_import() {
        let src = with_import("await page.pause();");
        let d = run("e2e/login.ts", &src);
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "playwright-no-page-pause");
    }

    #[test]
    fn allows_other_pause() {
        let src = with_import("await video.pause();");
        assert!(run("test.spec.ts", &src).is_empty());
    }

    #[test]
    fn ignores_without_playwright_import() {
        assert!(run("test.spec.ts", "await page.pause();").is_empty());
    }

    #[test]
    fn ignores_non_test_without_import() {
        assert!(run("src/utils.ts", "await page.pause();").is_empty());
    }
}
