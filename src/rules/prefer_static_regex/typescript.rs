use crate::diagnostic::{Diagnostic, Severity};

const TEST_MARKERS: &[&str] = &[".test.", ".spec.", "__tests__", "_test.", ".e2e."];

/// Test file: regex compilation cost in tests is irrelevant, and Playwright
/// locators (`page.getByRole('heading', { name: /Page introuvable/i })`)
/// would otherwise be flagged.
fn is_test_file(path: &std::path::Path) -> bool {
    let s = path.to_string_lossy();
    if TEST_MARKERS.iter().any(|m| s.contains(m)) {
        return true;
    }
    path.components()
        .any(|c| c.as_os_str() == "tests" || c.as_os_str() == "e2e")
}

crate::ast_check! { on ["regex"] => |node, source, ctx, diagnostics|
    let _ = source;
    if is_test_file(ctx.path) { return; }
    // Look for regex literals
    // Check if inside a function
    let mut current = node.parent();
    let mut inside_function = false;

    while let Some(parent) = current {
        match parent.kind() {
            "function_declaration" | "function_expression" | "arrow_function"
            | "method_definition" | "generator_function" | "generator_function_declaration" => {
                inside_function = true;
                break;
            }
            "program" | "class_body" => break,
            _ => {}
        }
        current = parent.parent();
    }

    if !inside_function { return; }

    // Skip if regex uses variables (new RegExp with template)
    // We only flag literal /.../ inside functions

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "prefer-static-regex".into(),
        message: "Regex literal inside function is recompiled on each call. Hoist to module scope.".into(),
        severity: Severity::Warning,
        span: None,
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    fn run(code: &str) -> Vec<Diagnostic> { crate::rules::test_helpers::run_ts(code, &Check) }
    fn run_at(code: &str, path: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts_with_path(code, &Check, path)
    }

    #[test]
    fn flags_regex_in_function() {
        assert_eq!(run("function f() { return /abc/.test(s); }").len(), 1);
        assert_eq!(run("const f = () => /abc/.test(s)").len(), 1);
    }

    #[test]
    fn flags_regex_in_method() {
        let code = "class C { m() { return /abc/.test(s); } }";
        assert_eq!(run(code).len(), 1);
    }

    #[test]
    fn allows_module_level_regex() {
        assert!(run("const RE = /abc/;").is_empty());
        assert!(run("const RE = /abc/g;").is_empty());
    }

    #[test]
    fn allows_class_property_regex() {
        assert!(run("class C { re = /abc/; }").is_empty());
    }

    #[test]
    fn allows_regex_in_test_file() {
        let code = "function f() { return /abc/.test(s); }";
        assert!(run_at(code, "src/foo.test.ts").is_empty());
        assert!(run_at(code, "src/foo.spec.ts").is_empty());
        assert!(run_at(code, "src/__tests__/foo.ts").is_empty());
        assert!(run_at(code, "e2e/foo.ts").is_empty());
        assert!(run_at(code, "tests/foo.ts").is_empty());
    }

    #[test]
    fn allows_playwright_locator_regex_in_test() {
        let code = r#"
test('shows 404', async ({ page }) => {
    await expect(page.getByRole('heading', { name: /Page introuvable/i })).toBeVisible();
});
"#;
        assert!(run_at(code, "e2e/not-found.spec.ts").is_empty());
    }
}
