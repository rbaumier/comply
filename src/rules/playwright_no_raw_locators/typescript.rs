//! playwright-no-raw-locators AST backend — flag `.locator()` with CSS selectors.

use crate::diagnostic::{Diagnostic, Severity};

const TEST_MARKERS: &[&str] = &[".test.", ".spec.", "__tests__", "_test.", ".e2e."];

fn is_test_file(path: &std::path::Path) -> bool {
    let s = path.to_string_lossy();
    TEST_MARKERS.iter().any(|m| s.contains(m))
}

/// Characters that indicate a CSS selector rather than a text/role locator.
const CSS_INDICATOR_CHARS: &[char] = &['.', '#', '[', '>', ':', '+', '~'];

crate::ast_check! { on ["call_expression"] => |node, source, ctx, diagnostics|
    if !is_test_file(ctx.path) {
        return;
    }
    let Some(callee) = node.child_by_field_name("function") else { return };
    if callee.kind() != "member_expression" {
        return;
    }

    let Some(prop) = callee.child_by_field_name("property") else { return };
    if prop.utf8_text(source).unwrap_or("") != "locator" {
        return;
    }

    // Extract the first argument (a string literal).
    let Some(args) = node.child_by_field_name("arguments") else { return };
    let mut cursor = args.walk();
    let first_arg = args.children(&mut cursor)
        .find(|c| c.kind() == "string" || c.kind() == "template_string");

    let Some(arg) = first_arg else { return };
    let text = arg.utf8_text(source).unwrap_or("");
    // Strip quotes.
    let inner = text.trim_matches(|c| c == '\'' || c == '"' || c == '`');
    if !inner.chars().any(|c| CSS_INDICATOR_CHARS.contains(&c)) {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "playwright-no-raw-locators".into(),
        message: "Raw CSS selector in `.locator()` — prefer \
                  `getByRole`, `getByText`, or other \
                  semantic locators."
            .into(),
        severity: Severity::Warning,
        span: None,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts_with_path(source, &Check, "login.test.ts")
    }

    #[test]
    fn flags_class_selector() {
        let d = run_on("page.locator('.submit-btn');");
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "playwright-no-raw-locators");
    }

    #[test]
    fn flags_id_selector() {
        let d = run_on("page.locator('#login-form');");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_text_locator() {
        assert!(run_on("page.locator('Submit');").is_empty());
    }

    #[test]
    fn allows_get_by_role() {
        assert!(run_on("page.getByRole('button');").is_empty());
    }

    #[test]
    fn ignores_non_test_file() {
        let d = crate::rules::test_helpers::run_ts_with_path(
            "page.locator('.btn');",
            &Check,
            "helpers.ts",
        );
        assert!(d.is_empty());
    }
}
