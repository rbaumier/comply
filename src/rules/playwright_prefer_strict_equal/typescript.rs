//! playwright-prefer-strict-equal — suggest `toStrictEqual()` over `toEqual()`.

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
    if prop.utf8_text(source).unwrap_or("") != "toEqual" {
        return;
    }

    // Verify the chain comes from expect().
    let Some(obj) = callee.child_by_field_name("object") else { return };
    if !is_expect_chain(obj, source) {
        return;
    }

    let pos = prop.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "playwright-prefer-strict-equal".into(),
        message: "Use toStrictEqual() instead.".into(),
        severity: Severity::Warning,
        span: None,
    });
}

fn is_expect_chain(node: tree_sitter::Node, source: &[u8]) -> bool {
    match node.kind() {
        "call_expression" => {
            if let Some(f) = node.child_by_field_name("function")
                && f.kind() == "identifier" && f.utf8_text(source).unwrap_or("") == "expect" {
                    return true;
                }
            false
        }
        "member_expression" => {
            if let Some(obj) = node.child_by_field_name("object") {
                is_expect_chain(obj, source)
            } else {
                false
            }
        }
        _ => false,
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
    fn flags_to_equal() {
        let d = run_ts("expect({a: 1}).toEqual({a: 1});");
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "playwright-prefer-strict-equal");
    }

    #[test]
    fn allows_to_strict_equal() {
        let d = run_ts("expect({a: 1}).toStrictEqual({a: 1});");
        assert!(d.is_empty());
    }

    #[test]
    fn allows_to_be() {
        let d = run_ts("expect(1).toBe(1);");
        assert!(d.is_empty());
    }
}
