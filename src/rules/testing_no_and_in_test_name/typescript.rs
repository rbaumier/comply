//! testing-no-and-in-test-name backend — flag `test("... and ...", …)`
//! / `it("... and ...", …)` names that combine multiple behaviors.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "call_expression" { return; }
    let path = ctx.path.to_string_lossy();
    if !path.contains(".test.") && !path.contains(".spec.") { return; }
    let Some(callee) = node.child_by_field_name("function") else { return; };
    let Ok(callee_name) = callee.utf8_text(source) else { return; };
    if callee_name != "test" && callee_name != "it" { return; }
    let Some(args) = node.child_by_field_name("arguments") else { return; };
    let mut cursor = args.walk();
    let Some(first) = args.children(&mut cursor).find(|c| {
        matches!(c.kind(), "string" | "template_string")
    }) else { return; };
    let Ok(raw) = first.utf8_text(source) else { return; };
    let unquoted = raw.trim_matches(|c| c == '"' || c == '\'' || c == '`');
    if !unquoted.contains(" and ") { return; }
    let pos = first.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "testing-no-and-in-test-name".into(),
        message: format!(
            "Test name {unquoted:?} contains \" and \" — split into two focused tests."
        ),
        severity: Severity::Warning,
        span: None,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts_with_path(s, &Check, "foo.test.ts")
    }

    #[test]
    fn flags_and_in_test_name() {
        assert_eq!(
            run("test('validates email and sends confirmation', () => {})").len(),
            1
        );
    }

    #[test]
    fn allows_single_behavior() {
        assert!(run("test('validates email format', () => {})").is_empty());
    }
}
