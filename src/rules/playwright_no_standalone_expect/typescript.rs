//! playwright-no-standalone-expect — disallow `expect` outside test blocks.

use crate::diagnostic::{Diagnostic, Severity};

const TEST_MARKERS: &[&str] = &[".test.", ".spec.", "__tests__", "_test.", ".e2e."];

fn is_test_file(path: &std::path::Path) -> bool {
    let s = path.to_string_lossy();
    TEST_MARKERS.iter().any(|m| s.contains(m))
}

const TEST_FNS: &[&str] = &["test", "it"];
const HOOK_FNS: &[&str] = &["beforeAll", "beforeEach", "afterAll", "afterEach"];

/// Walk ancestors to check if this expect is inside a test or hook callback.
fn is_inside_test_or_hook(node: tree_sitter::Node, source: &[u8]) -> bool {
    let mut cur = node.parent();
    while let Some(p) = cur {
        match p.kind() {
            "arrow_function" | "function_expression" | "function" => {
                if let Some(pp) = p.parent()
                    && pp.kind() == "arguments"
                        && let Some(call) = pp.parent()
                            && call.kind() == "call_expression"
                                && let Some(callee) = call.child_by_field_name("function") {
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
                                    if TEST_FNS.contains(&name) || HOOK_FNS.contains(&name) {
                                        return true;
                                    }
                                }
            }
            _ => {}
        }
        cur = p.parent();
    }
    false
}

fn is_expect_call(node: tree_sitter::Node, source: &[u8]) -> bool {
    if node.kind() != "call_expression" {
        return false;
    }
    let Some(callee) = node.child_by_field_name("function") else { return false };
    // Direct expect(...)
    if callee.kind() == "identifier" && callee.utf8_text(source).unwrap_or("") == "expect" {
        return true;
    }
    // expect.soft(...) — member expression where object is the `expect` identifier
    if callee.kind() == "member_expression"
        && let Some(obj) = callee.child_by_field_name("object")
            && obj.kind() == "identifier" && obj.utf8_text(source).unwrap_or("") == "expect" {
                return true;
            }
    false
}

crate::ast_check! { |node, source, ctx, diagnostics|
    if !is_test_file(ctx.path) {
        return;
    }

    if !is_expect_call(node, source) {
        return;
    }

    if is_inside_test_or_hook(node, source) {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "playwright-no-standalone-expect".into(),
        message: "Expect must be inside of a test block.".into(),
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
    fn flags_standalone_expect() {
        let d = run_ts("expect(1).toBe(1);");
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "playwright-no-standalone-expect");
    }

    #[test]
    fn allows_expect_in_test() {
        let d = run_ts("test('ok', () => { expect(1).toBe(1); });");
        assert!(d.is_empty());
    }

    #[test]
    fn allows_expect_in_hook() {
        let d = run_ts("beforeEach(() => { expect(setup).toBeDefined(); });");
        assert!(d.is_empty());
    }
}
