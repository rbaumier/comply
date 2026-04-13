use crate::diagnostic::{Diagnostic, Severity};

/// Deprecated direct page methods we want to flag.
const DEPRECATED_METHODS: &[&str] = &["click", "fill", "type", "check", "uncheck"];

/// Path fragments that identify test files.
const TEST_MARKERS: &[&str] = &[".test.", ".spec.", "__tests__", "_test.", ".e2e."];

crate::ast_check! { |node, source, ctx, diagnostics|
    // Only fire in test files.
    let path_str = ctx.path.to_string_lossy();
    if !TEST_MARKERS.iter().any(|m| path_str.contains(m)) {
        return;
    }

    if node.kind() != "call_expression" {
        return;
    }

    let Some(callee) = node.child_by_field_name("function") else { return };
    if callee.kind() != "member_expression" {
        return;
    }

    let Some(obj) = callee.child_by_field_name("object") else { return };
    if obj.utf8_text(source).unwrap_or("") != "page" {
        return;
    }

    let Some(prop) = callee.child_by_field_name("property") else { return };
    let method = prop.utf8_text(source).unwrap_or("");
    if !DEPRECATED_METHODS.contains(&method) {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "no-page-click-deprecated".into(),
        message: format!(
            "Deprecated Playwright method `page.{method}()` — use \
             `page.locator(selector).{method}()` instead.",
        ),
        severity: Severity::Warning,
        span: None,
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    use crate::rules::backend::{AstCheck, CheckCtx};

    fn run(path: &str, source: &str) -> Vec<Diagnostic> {
        let mut parser = tree_sitter::Parser::new();
        parser.set_language(&tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into()).unwrap();
        let tree = parser.parse(source, None).unwrap();
        Check.check(&CheckCtx::for_test(Path::new(path), source), &tree)
    }

    #[test]
    fn flags_page_click_in_test() {
        let d = run("login.test.ts", "await page.click('#btn');");
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "no-page-click-deprecated");
    }

    #[test]
    fn flags_page_fill_in_spec() {
        let d = run("form.spec.ts", "await page.fill('#email', 'a@b.c');");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_locator_api() {
        let d = run("login.test.ts", "await page.locator('#btn').click();");
        assert!(d.is_empty());
    }

    #[test]
    fn ignores_non_test_file() {
        let d = run("utils.ts", "await page.click('#btn');");
        assert!(d.is_empty());
    }
}
