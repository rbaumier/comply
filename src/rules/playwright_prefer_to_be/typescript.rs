//! playwright-prefer-to-be — suggest `toBe()` for primitive literals.

use crate::diagnostic::{Diagnostic, Severity};

const TEST_MARKERS: &[&str] = &[".test.", ".spec.", "__tests__", "_test.", ".e2e."];

fn is_test_file(path: &std::path::Path) -> bool {
    let s = path.to_string_lossy();
    TEST_MARKERS.iter().any(|m| s.contains(m))
}

const EQUALITY_MATCHERS: &[&str] = &["toEqual", "toStrictEqual"];

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

/// Check if an argument is a primitive literal (string, number, boolean, null).
fn is_primitive_literal(node: tree_sitter::Node, source: &[u8]) -> bool {
    match node.kind() {
        "string" | "number" | "true" | "false" | "null" | "template_string" => true,
        "unary_expression" => {
            // -1, +2
            if let Some(arg) = node.named_child(0) {
                arg.kind() == "number"
            } else {
                false
            }
        }
        "identifier" => {
            let text = node.utf8_text(source).unwrap_or("");
            text == "undefined" || text == "NaN"
        }
        _ => false,
    }
}

fn suggested_matcher(node: tree_sitter::Node, source: &[u8]) -> &'static str {
    match node.kind() {
        "null" => "toBeNull",
        "identifier" => {
            let text = node.utf8_text(source).unwrap_or("");
            match text {
                "undefined" => "toBeUndefined",
                "NaN" => "toBeNaN",
                _ => "toBe",
            }
        }
        _ => "toBe",
    }
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
    let matcher = prop.utf8_text(source).unwrap_or("");
    if !EQUALITY_MATCHERS.contains(&matcher) {
        return;
    }

    let Some(obj) = callee.child_by_field_name("object") else { return };
    if !is_expect_chain(obj, source) {
        return;
    }

    let Some(args) = node.child_by_field_name("arguments") else { return };
    let Some(arg) = args.named_child(0) else { return };

    if !is_primitive_literal(arg, source) {
        return;
    }

    let suggested = suggested_matcher(arg, source);
    let pos = prop.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "playwright-prefer-to-be".into(),
        message: format!("Use `{suggested}` when expecting primitive literals."),
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
    fn flags_to_equal_with_number() {
        let d = run_ts("expect(x).toEqual(1);");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("toBe"));
    }

    #[test]
    fn flags_to_equal_null() {
        let d = run_ts("expect(x).toEqual(null);");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("toBeNull"));
    }

    #[test]
    fn allows_to_equal_with_object() {
        let d = run_ts("expect(x).toEqual({a: 1});");
        assert!(d.is_empty());
    }

    #[test]
    fn allows_to_be() {
        let d = run_ts("expect(x).toBe(1);");
        assert!(d.is_empty());
    }
}
