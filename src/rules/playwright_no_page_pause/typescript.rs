//! playwright-no-page-pause — flag `page.pause()` debug-only API.

use crate::diagnostic::{Diagnostic, Severity};

const TEST_MARKERS: &[&str] = &[".test.", ".spec.", "__tests__", "_test.", ".e2e."];

fn is_test_file(path: &std::path::Path) -> bool {
    let s = path.to_string_lossy();
    TEST_MARKERS.iter().any(|m| s.contains(m))
}

crate::ast_check! { on ["call_expression"] => |node, source, ctx, diagnostics|
    if !is_test_file(ctx.path) {
        return;
    }
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
    fn flags_page_pause() {
        let d = run("login.test.ts", "await page.pause();");
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "playwright-no-page-pause");
    }

    #[test]
    fn flags_page_pause_in_spec() {
        let d = run("checkout.spec.ts", "  await page.pause();");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_other_pause() {
        let d = run("login.test.ts", "await video.pause();");
        assert!(d.is_empty());
    }

    #[test]
    fn ignores_non_test_file() {
        let d = run("helpers.ts", "await page.pause();");
        assert!(d.is_empty());
    }
}
