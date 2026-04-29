use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["try_statement"] prefilter = ["new URL"] => |node, source, ctx, diagnostics|
    // Look for try statements
    let Some(body) = node.child_by_field_name("body") else { return; };

    // Check if the try block contains only `new URL(...)` or a variable assignment with `new URL(...)`
    let body_text = body.utf8_text(source).unwrap_or("");

    // Simple heuristic: try block contains "new URL(" and catch block exists
    if !body_text.contains("new URL(") { return; }

    // Check there's a catch clause (validation pattern)
    let has_catch = node.child_by_field_name("handler").is_some();
    if !has_catch { return; }

    // Check if this is likely a validation pattern (return true/false or assign boolean)
    let catch_body = node.child_by_field_name("handler")
        .and_then(|h| h.child_by_field_name("body"))
        .map(|b| b.utf8_text(source).unwrap_or(""))
        .unwrap_or("");

    let is_validation_pattern = body_text.contains("return true")
        || body_text.contains("return new URL")
        || catch_body.contains("return false")
        || catch_body.contains("return null")
        || catch_body.contains("return undefined");

    if !is_validation_pattern { return; }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "prefer-url-canparse".into(),
        message: "Use `URL.canParse(url)` instead of try-catch with `new URL()`.".into(),
        severity: Severity::Warning,
        span: None,
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    fn run(code: &str) -> Vec<Diagnostic> { crate::rules::test_helpers::run_ts(code, &Check) }

    #[test]
    fn flags_try_catch_url_validation() {
        let code = r#"
            function isValidUrl(url) {
                try { new URL(url); return true; }
                catch { return false; }
            }
        "#;
        assert_eq!(run(code).len(), 1);
    }

    #[test]
    fn flags_try_catch_return_null() {
        let code = r#"
            function parseUrl(url) {
                try { return new URL(url); }
                catch { return null; }
            }
        "#;
        assert_eq!(run(code).len(), 1);
    }

    #[test]
    fn allows_url_canparse() {
        assert!(run("const valid = URL.canParse(url);").is_empty());
    }

    #[test]
    fn allows_try_catch_without_validation_return() {
        let code = r#"
            try { const u = new URL(url); process(u); }
            catch (e) { console.error(e); }
        "#;
        assert!(run(code).is_empty());
    }
}
