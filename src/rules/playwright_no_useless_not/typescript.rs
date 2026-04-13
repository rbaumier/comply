//! playwright-no-useless-not — disallow `not` when a direct matcher exists.

use crate::diagnostic::{Diagnostic, Severity};

const TEST_MARKERS: &[&str] = &[".test.", ".spec.", "__tests__", "_test.", ".e2e."];

fn is_test_file(path: &std::path::Path) -> bool {
    let s = path.to_string_lossy();
    TEST_MARKERS.iter().any(|m| s.contains(m))
}

/// Matchers that have a direct inverse.
const MATCHER_PAIRS: &[(&str, &str)] = &[
    ("toBeVisible", "toBeHidden"),
    ("toBeHidden", "toBeVisible"),
    ("toBeEnabled", "toBeDisabled"),
    ("toBeDisabled", "toBeEnabled"),
];

fn inverse_of(matcher: &str) -> Option<&'static str> {
    MATCHER_PAIRS.iter().find(|(m, _)| *m == matcher).map(|(_, inv)| *inv)
}

// Check: expect(…).not.toBeVisible() etc.
crate::ast_check! { |node, source, ctx, diagnostics|
    if !is_test_file(ctx.path) {
        return;
    }

    if node.kind() != "call_expression" {
        return;
    }

    // Pattern: expect(x).not.toBeVisible()
    // AST: call_expression { function: member_expression { object: member_expression { object: call_expression(expect), property: "not" }, property: "toBeVisible" } }
    let Some(callee) = node.child_by_field_name("function") else { return };
    if callee.kind() != "member_expression" {
        return;
    }
    let Some(matcher_prop) = callee.child_by_field_name("property") else { return };
    let matcher_name = matcher_prop.utf8_text(source).unwrap_or("");

    let Some(inverse) = inverse_of(matcher_name) else { return };

    let Some(not_member) = callee.child_by_field_name("object") else { return };
    if not_member.kind() != "member_expression" {
        return;
    }
    let Some(not_prop) = not_member.child_by_field_name("property") else { return };
    if not_prop.utf8_text(source).unwrap_or("") != "not" {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "playwright-no-useless-not".into(),
        message: format!("Unexpected usage of not.{matcher_name}(). Use {inverse}() instead."),
        severity: Severity::Warning,
        span: None,
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::test_helpers::run_ts_with_path;

    fn run_ts(source: &str) -> Vec<Diagnostic> {
        run_ts_with_path(source, &Check, "app.test.ts")
    }

    #[test]
    fn flags_not_to_be_visible() {
        let d = run_ts("await expect(el).not.toBeVisible();");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("toBeHidden"));
    }

    #[test]
    fn flags_not_to_be_enabled() {
        let d = run_ts("await expect(el).not.toBeEnabled();");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("toBeDisabled"));
    }

    #[test]
    fn allows_not_to_be() {
        let d = run_ts("await expect(el).not.toBe(1);");
        assert!(d.is_empty());
    }
}
