//! playwright-no-skipped-test — disallow `.skip()` test annotation.

use crate::diagnostic::{Diagnostic, Severity};

const TEST_MARKERS: &[&str] = &[".test.", ".spec.", "__tests__", "_test.", ".e2e."];

fn is_test_file(path: &std::path::Path) -> bool {
    let s = path.to_string_lossy();
    TEST_MARKERS.iter().any(|m| s.contains(m))
}

/// Test/describe function names.
const TEST_FNS: &[&str] = &["test", "it", "describe"];

crate::ast_check! { on ["call_expression"] => |node, source, ctx, diagnostics|
    if !is_test_file(ctx.path) {
        return;
    }
    let Some(callee) = node.child_by_field_name("function") else { return };
    if callee.kind() != "member_expression" {
        return;
    }

    let Some(obj) = callee.child_by_field_name("object") else { return };
    let Some(prop) = callee.child_by_field_name("property") else { return };

    let obj_text = obj.utf8_text(source).unwrap_or("");
    let prop_text = prop.utf8_text(source).unwrap_or("");

    // Match test.skip(...), describe.skip(...), it.skip(...)
    if TEST_FNS.contains(&obj_text) && prop_text == "skip" {
        let pos = node.start_position();
        diagnostics.push(Diagnostic {
            path: ctx.path.to_path_buf(),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "playwright-no-skipped-test".into(),
            message: "Unexpected use of the `.skip()` annotation.".into(),
            severity: Severity::Warning,
            span: None,
        });
        return;
    }

    // Also check chained: test.skip.each(...)  — obj is member_expression
    if obj.kind() == "member_expression"
        && let Some(inner_obj) = obj.child_by_field_name("object")
            && let Some(inner_prop) = obj.child_by_field_name("property") {
                let inner_obj_text = inner_obj.utf8_text(source).unwrap_or("");
                let inner_prop_text = inner_prop.utf8_text(source).unwrap_or("");
                if TEST_FNS.contains(&inner_obj_text) && inner_prop_text == "skip" {
                    let pos = node.start_position();
                    diagnostics.push(Diagnostic {
                        path: ctx.path.to_path_buf(),
                        line: pos.row + 1,
                        column: pos.column + 1,
                        rule_id: "playwright-no-skipped-test".into(),
                        message: "Unexpected use of the `.skip()` annotation.".into(),
                        severity: Severity::Warning,
                        span: None,
                    });
                }
            }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::test_helpers::run_ts_with_path;

    fn run_ts(source: &str) -> Vec<Diagnostic> {
        run_ts_with_path(source, &Check, "app.test.ts")
    }

    #[test]
    fn flags_test_skip() {
        let d = run_ts("test.skip('broken', () => {});");
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "playwright-no-skipped-test");
    }

    #[test]
    fn flags_describe_skip() {
        let d = run_ts("describe.skip('suite', () => {});");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_test_only() {
        let d = run_ts("test.only('focused', () => {});");
        assert!(d.is_empty());
    }
}
