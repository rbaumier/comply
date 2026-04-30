//! playwright-no-conditional-in-test — disallow conditional logic inside test bodies.

use crate::diagnostic::{Diagnostic, Severity};

const TEST_MARKERS: &[&str] = &[".test.", ".spec.", "__tests__", "_test.", ".e2e."];

fn is_test_file(path: &std::path::Path) -> bool {
    let s = path.to_string_lossy();
    TEST_MARKERS.iter().any(|m| s.contains(m))
}

const TEST_FNS: &[&str] = &["test", "it"];

const CONDITIONAL_KINDS: &[&str] = &["if_statement", "switch_statement", "ternary_expression"];

/// Walk up from `node` to check if it's inside a test callback.
fn is_inside_test_callback(node: tree_sitter::Node, source: &[u8]) -> bool {
    let mut cur = node.parent();
    let mut found_function = false;
    while let Some(p) = cur {
        match p.kind() {
            "arrow_function" | "function_expression" | "function" => {
                found_function = true;
            }
            "call_expression" if found_function => {
                if let Some(callee) = p.child_by_field_name("function") {
                    let name = match callee.kind() {
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
                    if TEST_FNS.contains(&name) {
                        return true;
                    }
                }
                found_function = false;
            }
            _ => {}
        }
        cur = p.parent();
    }
    false
}

crate::ast_check! { |node, source, ctx, diagnostics|
    if !is_test_file(ctx.path) {
        return;
    }
    if !source.windows(16).any(|w| w == b"@playwright/test") {
        return;
    }

    if !CONDITIONAL_KINDS.contains(&node.kind()) {
        return;
    }

    if !is_inside_test_callback(node, source) {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "playwright-no-conditional-in-test".into(),
        message: "Avoid having conditionals in tests.".into(),
        severity: Severity::Warning,
        span: None,
    });
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
    fn flags_if_in_test() {
        let d = run_ts("test('cond', () => { if (x) { expect(1).toBe(1); } });");
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "playwright-no-conditional-in-test");
    }

    #[test]
    fn allows_if_outside_test() {
        let d = run_ts("if (process.env.CI) { console.log('ci'); }");
        assert!(d.is_empty());
    }

    #[test]
    fn flags_ternary_in_test() {
        let d = run_ts("test('tern', () => { const v = x ? 1 : 2; });");
        assert_eq!(d.len(), 1);
    }
}
