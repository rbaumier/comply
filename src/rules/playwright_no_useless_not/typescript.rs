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
    MATCHER_PAIRS
        .iter()
        .find(|(m, _)| *m == matcher)
        .map(|(_, inv)| *inv)
}

// Check: expect(…).not.toBeVisible() etc.
crate::ast_check! { on ["call_expression"] => |node, source, ctx, diagnostics|
    if !is_test_file(ctx.path) {
        return;
    }
    if !crate::rules::playwright::is_playwright_context(ctx) {
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
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "playwright-no-useless-not".into(),
        message: format!("Unexpected usage of not.{matcher_name}(). Use {inverse}() instead."),
        severity: Severity::Warning,
        span: None,
    });
}


#[cfg(test)]
impl crate::rules::test_helpers::RunRule for Check {
    fn meta(&self) -> &'static crate::rules::meta::RuleMeta {
        &super::META
    }
    fn execute_with_ctx(
        &self,
        src: &str,
        path: &std::path::Path,
        project: &crate::project::ProjectCtx,
        file: &crate::rules::file_ctx::FileCtx,
    ) -> Vec<crate::diagnostic::Diagnostic> {
        crate::rules::test_helpers::run_ast_check(self, src, path, project, file)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    
    const PW_IMPORT: &str = "import { test, expect } from \"@playwright/test\";\n";

    fn run_ts(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, &format!("{PW_IMPORT}{source}"), "app.test.ts")
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
