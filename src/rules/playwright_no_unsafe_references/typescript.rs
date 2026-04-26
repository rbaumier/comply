//! playwright-no-unsafe-references AST backend — flag `page.evaluate()` with
//! a single function argument that likely captures outer-scope variables.

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

    let Some(prop) = callee.child_by_field_name("property") else { return };
    if prop.utf8_text(source).unwrap_or("") != "evaluate" {
        return;
    }

    // Check the receiver is `page` (or any object — the text version
    // only checked `page.evaluate`).
    let Some(obj) = callee.child_by_field_name("object") else { return };
    if obj.utf8_text(source).unwrap_or("") != "page" {
        return;
    }

    // Count non-punctuation arguments.
    let Some(args) = node.child_by_field_name("arguments") else { return };
    let mut cursor = args.walk();
    let real_args: Vec<_> = args.children(&mut cursor)
        .filter(|c| !matches!(c.kind(), "(" | ")" | ","))
        .collect();

    // Must have exactly one argument and it must be an arrow function or function.
    if real_args.len() != 1 {
        return;
    }
    let arg = real_args[0];
    if !matches!(arg.kind(), "arrow_function" | "function_expression" | "function") {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "playwright-no-unsafe-references".into(),
        message: "`page.evaluate()` with a single function \
                  argument — pass captured variables as the \
                  second argument."
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
    fn flags_evaluate_with_single_arrow() {
        let d = run_on("await page.evaluate(() => document.title);");
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "playwright-no-unsafe-references");
    }

    #[test]
    fn flags_evaluate_with_arrow_body() {
        let d = run_on("await page.evaluate(() => { return window.scrollY; });");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_evaluate_with_second_arg() {
        let d = run_on("await page.evaluate((name) => document.title + name, userName);");
        assert!(d.is_empty());
    }

    #[test]
    fn allows_evaluate_with_string_arg() {
        let d = run_on("await page.evaluate('document.title');");
        assert!(d.is_empty());
    }

    #[test]
    fn ignores_non_test_file() {
        let d = crate::rules::test_helpers::run_ts_with_path(
            "await page.evaluate(() => document.title);",
            &Check,
            "helpers.ts",
        );
        assert!(d.is_empty());
    }
}
