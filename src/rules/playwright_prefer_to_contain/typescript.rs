//! playwright-prefer-to-contain — suggest `toContain` over `includes()` + equality.

use crate::diagnostic::{Diagnostic, Severity};

const TEST_MARKERS: &[&str] = &[".test.", ".spec.", "__tests__", "_test.", ".e2e."];

fn is_test_file(path: &std::path::Path) -> bool {
    let s = path.to_string_lossy();
    TEST_MARKERS.iter().any(|m| s.contains(m))
}

const EQUALITY_MATCHERS: &[&str] = &["toBe", "toEqual", "toStrictEqual"];

crate::ast_check! { on ["call_expression"] => |node, source, ctx, diagnostics|
    if !is_test_file(ctx.path) {
        return;
    }
    // Pattern: expect(arr.includes(x)).toBe(true)
    let Some(callee) = node.child_by_field_name("function") else { return };
    if callee.kind() != "member_expression" {
        return;
    }

    let Some(matcher_prop) = callee.child_by_field_name("property") else { return };
    let matcher = matcher_prop.utf8_text(source).unwrap_or("");
    if !EQUALITY_MATCHERS.contains(&matcher) {
        return;
    }

    // The arg should be true or false
    let Some(args) = node.child_by_field_name("arguments") else { return };
    let Some(arg) = args.named_child(0) else { return };
    let arg_text = arg.utf8_text(source).unwrap_or("");
    if arg_text != "true" && arg_text != "false" {
        return;
    }

    // The object should be expect(…) call
    let Some(expect_call) = callee.child_by_field_name("object") else { return };
    if expect_call.kind() != "call_expression" {
        return;
    }
    let Some(expect_fn) = expect_call.child_by_field_name("function") else { return };
    if expect_fn.utf8_text(source).unwrap_or("") != "expect" {
        return;
    }

    // The argument to expect should be a .includes() call
    let Some(expect_args) = expect_call.child_by_field_name("arguments") else { return };
    let Some(includes_call) = expect_args.named_child(0) else { return };
    if includes_call.kind() != "call_expression" {
        return;
    }

    let Some(includes_callee) = includes_call.child_by_field_name("function") else { return };
    if includes_callee.kind() != "member_expression" {
        return;
    }

    let Some(includes_prop) = includes_callee.child_by_field_name("property") else { return };
    if includes_prop.utf8_text(source).unwrap_or("") != "includes" {
        return;
    }

    let pos = matcher_prop.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "playwright-prefer-to-contain".into(),
        message: "Prefer using `toContain()` instead.".into(),
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
    fn flags_includes_to_be_true() {
        let d = run_ts("expect(arr.includes(1)).toBe(true);");
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "playwright-prefer-to-contain");
    }

    #[test]
    fn flags_includes_to_equal_false() {
        let d = run_ts("expect(arr.includes(1)).toEqual(false);");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_to_contain() {
        let d = run_ts("expect(arr).toContain(1);");
        assert!(d.is_empty());
    }
}
