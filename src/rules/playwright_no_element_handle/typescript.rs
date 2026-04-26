//! playwright-no-element-handle — flag `page.$()` and `page.$$()`.

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
    let Some(callee) = node.child_by_field_name("function") else { return };
    if callee.kind() != "member_expression" {
        return;
    }

    let Some(object) = callee.child_by_field_name("object") else { return };
    let Some(property) = callee.child_by_field_name("property") else { return };

    let obj_text = object.utf8_text(source).unwrap_or("");
    let prop_text = property.utf8_text(source).unwrap_or("");

    // page.$() or page.$$()
    if obj_text.contains("page") && (prop_text == "$" || prop_text == "$$") {
        let label = if prop_text == "$$" { "page.$$()" } else { "page.$()" };
        let pos = node.start_position();
        diagnostics.push(Diagnostic {
            path: ctx.path.to_path_buf(),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "playwright-no-element-handle".into(),
            message: format!(
                "`{label}` returns a deprecated ElementHandle — use `page.locator()` instead."
            ),
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
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into())
            .unwrap();
        let tree = parser.parse(source, None).unwrap();
        Check.check(&CheckCtx::for_test(Path::new(path), source), &tree)
    }

    #[test]
    fn flags_page_dollar() {
        let d = run("login.test.ts", "const el = await page.$('.btn');");
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "playwright-no-element-handle");
    }

    #[test]
    fn flags_page_dollar_dollar() {
        let d = run("list.spec.ts", "const items = await page.$$('li');");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_page_locator() {
        let d = run("login.test.ts", "const el = page.locator('.btn');");
        assert!(d.is_empty());
    }

    #[test]
    fn ignores_non_test_file() {
        let d = run("helpers.ts", "const el = await page.$('.btn');");
        assert!(d.is_empty());
    }
}
