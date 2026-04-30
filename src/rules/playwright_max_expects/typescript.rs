//! playwright-max-expects — limit assertion count per test body.

use crate::diagnostic::{Diagnostic, Severity};

const TEST_MARKERS: &[&str] = &[".test.", ".spec.", "__tests__", "_test.", ".e2e."];

fn is_test_file(path: &std::path::Path) -> bool {
    let s = path.to_string_lossy();
    TEST_MARKERS.iter().any(|m| s.contains(m))
}

const TEST_FNS: &[&str] = &["test", "it"];

fn is_test_call(node: tree_sitter::Node, source: &[u8]) -> bool {
    if node.kind() != "call_expression" {
        return false;
    }
    let Some(callee) = node.child_by_field_name("function") else {
        return false;
    };
    match callee.kind() {
        "identifier" => TEST_FNS.contains(&callee.utf8_text(source).unwrap_or("")),
        "member_expression" => {
            let Some(obj) = callee.child_by_field_name("object") else {
                return false;
            };
            TEST_FNS.contains(&obj.utf8_text(source).unwrap_or(""))
        }
        _ => false,
    }
}

fn count_expects(node: tree_sitter::Node, source: &[u8]) -> usize {
    let mut count = 0;
    if node.kind() == "call_expression"
        && let Some(callee) = node.child_by_field_name("function")
    {
        let text = match callee.kind() {
            "identifier" => callee.utf8_text(source).unwrap_or(""),
            "member_expression" => {
                if let Some(obj) = callee.child_by_field_name("object") {
                    obj.utf8_text(source).unwrap_or("")
                } else {
                    ""
                }
            }
            _ => "",
        };
        if text == "expect" {
            count += 1;
        }
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        count += count_expects(child, source);
    }
    count
}

crate::ast_check! { |node, source, ctx, diagnostics|
    if !is_test_file(ctx.path) {
        return;
    }
    if !source.windows(16).any(|w| w == b"@playwright/test") {
        return;
    }

    if !is_test_call(node, source) {
        return;
    }

    let Some(args) = node.child_by_field_name("arguments") else { return };
    let arg_count = args.named_child_count();
    if arg_count == 0 {
        return;
    }
    let Some(callback) = args.named_child(arg_count - 1) else { return };

    let max_expects = ctx.config.threshold("playwright-max-expects", "max");
    let count = count_expects(callback, source);
    if count > max_expects {
        let pos = node.start_position();
        diagnostics.push(Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "playwright-max-expects".into(),
            message: format!("Too many assertion calls ({count}) — maximum allowed is {max_expects}."),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::test_helpers::run_ts_with_path;

    const PW_IMPORT: &str = "import { test, expect } from \"@playwright/test\";\n";

    fn run_ts(source: &str) -> Vec<Diagnostic> {
        run_ts_with_path(&format!("{PW_IMPORT}{source}"), &Check, "login.test.ts")
    }

    #[test]
    fn flags_too_many_expects() {
        let src = "test('many', () => {
            expect(1).toBe(1);
            expect(2).toBe(2);
            expect(3).toBe(3);
            expect(4).toBe(4);
            expect(5).toBe(5);
            expect(6).toBe(6);
        });";
        let d = run_ts(src);
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "playwright-max-expects");
    }

    #[test]
    fn allows_five_expects() {
        let src = "test('ok', () => {
            expect(1).toBe(1);
            expect(2).toBe(2);
            expect(3).toBe(3);
            expect(4).toBe(4);
            expect(5).toBe(5);
        });";
        let d = run_ts(src);
        assert!(d.is_empty());
    }
}
