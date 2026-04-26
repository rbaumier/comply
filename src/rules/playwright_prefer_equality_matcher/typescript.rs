//! playwright-prefer-equality-matcher — suggest built-in equality matchers.

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
    let Some(callee) = node.child_by_field_name("function") else { return };
    if callee.kind() != "member_expression" {
        return;
    }

    let Some(matcher_prop) = callee.child_by_field_name("property") else { return };
    let matcher = matcher_prop.utf8_text(source).unwrap_or("");
    if !EQUALITY_MATCHERS.contains(&matcher) {
        return;
    }

    // The matcher arg must be true/false
    let Some(args) = node.child_by_field_name("arguments") else { return };
    let Some(arg) = args.named_child(0) else { return };
    let arg_text = arg.utf8_text(source).unwrap_or("");
    if arg_text != "true" && arg_text != "false" {
        return;
    }

    // The object should be expect(binary_expression) with === or !==
    let Some(expect_call) = callee.child_by_field_name("object") else { return };
    if expect_call.kind() != "call_expression" {
        return;
    }
    let Some(expect_fn) = expect_call.child_by_field_name("function") else { return };
    if expect_fn.utf8_text(source).unwrap_or("") != "expect" {
        return;
    }

    let Some(expect_args) = expect_call.child_by_field_name("arguments") else { return };
    let Some(comparison) = expect_args.named_child(0) else { return };
    if comparison.kind() != "binary_expression" {
        return;
    }

    // Check for === or !==
    let mut has_equality_op = false;
    let mut cursor = comparison.walk();
    for child in comparison.children(&mut cursor) {
        if !child.is_named() {
            let t = child.utf8_text(source).unwrap_or("");
            if t == "===" || t == "!==" {
                has_equality_op = true;
                break;
            }
        }
    }

    if !has_equality_op {
        return;
    }

    let pos = matcher_prop.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "playwright-prefer-equality-matcher".into(),
        message: "Prefer using one of the equality matchers instead.".into(),
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
    fn flags_strict_equality() {
        let d = run_ts("expect(a === b).toBe(true);");
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "playwright-prefer-equality-matcher");
    }

    #[test]
    fn flags_strict_inequality() {
        let d = run_ts("expect(a !== b).toEqual(true);");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_direct_equality() {
        let d = run_ts("expect(a).toBe(b);");
        assert!(d.is_empty());
    }
}
