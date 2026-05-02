//! playwright-prefer-hooks-on-top — hooks should come before test cases.

use crate::diagnostic::{Diagnostic, Severity};

const TEST_MARKERS: &[&str] = &[".test.", ".spec.", "__tests__", "_test.", ".e2e."];

fn is_test_file(path: &std::path::Path) -> bool {
    let s = path.to_string_lossy();
    TEST_MARKERS.iter().any(|m| s.contains(m))
}

const TEST_FNS: &[&str] = &["test", "it"];
const HOOKS: &[&str] = &["beforeAll", "beforeEach", "afterAll", "afterEach"];

fn get_call_name<'a>(node: tree_sitter::Node<'a>, source: &'a [u8]) -> Option<&'a str> {
    if node.kind() != "call_expression" {
        return None;
    }
    let callee = node.child_by_field_name("function")?;
    match callee.kind() {
        "identifier" => callee.utf8_text(source).ok(),
        "member_expression" => callee.child_by_field_name("object")?.utf8_text(source).ok(),
        _ => None,
    }
}

fn check_block(
    node: tree_sitter::Node,
    source: &[u8],
    ctx: &crate::rules::backend::CheckCtx,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let mut seen_test = false;
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "expression_statement"
            && let Some(expr) = child.named_child(0)
        {
            if let Some(name) = get_call_name(expr, source) {
                if TEST_FNS.contains(&name) {
                    seen_test = true;
                } else if HOOKS.contains(&name) && seen_test {
                    let pos = expr.start_position();
                    diagnostics.push(Diagnostic {
                        path: std::sync::Arc::clone(&ctx.path_arc),
                        line: pos.row + 1,
                        column: pos.column + 1,
                        rule_id: "playwright-prefer-hooks-on-top".into(),
                        message: "Hooks should come before test cases.".into(),
                        severity: Severity::Warning,
                        span: None,
                    });
                }
            }
            // Recurse into describe callbacks.
            if expr.kind() == "call_expression"
                && let Some(callee) = expr.child_by_field_name("function")
            {
                let is_describe = match callee.kind() {
                    "identifier" => callee.utf8_text(source).unwrap_or("") == "describe",
                    "member_expression" => {
                        callee
                            .child_by_field_name("object")
                            .and_then(|o| o.utf8_text(source).ok())
                            .unwrap_or("")
                            == "describe"
                    }
                    _ => false,
                };
                if is_describe && let Some(args) = expr.child_by_field_name("arguments") {
                    let ac = args.named_child_count();
                    if ac > 0
                        && let Some(cb) = args.named_child(ac - 1)
                        && let Some(body) = cb.child_by_field_name("body")
                    {
                        check_block(body, source, ctx, diagnostics);
                    }
                }
            }
        }
    }
}

crate::ast_check! { on ["program"] => |node, source, ctx, diagnostics|
    if !is_test_file(ctx.path) {
        return;
    }
    if !crate::rules::playwright::is_playwright_context(ctx) {
        return;
    }
    check_block(node, source, ctx, diagnostics);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::test_helpers::run_ts_with_path;

    const PW_IMPORT: &str = "import { test, expect } from \"@playwright/test\";\n";

    fn run_ts(source: &str) -> Vec<Diagnostic> {
        run_ts_with_path(&format!("{PW_IMPORT}{source}"), &Check, "app.test.ts")
    }

    #[test]
    fn flags_hook_after_test() {
        let src = "\
test('a', () => {});
beforeEach(() => {});";
        let d = run_ts(src);
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "playwright-prefer-hooks-on-top");
    }

    #[test]
    fn allows_hooks_before_tests() {
        let src = "\
beforeEach(() => {});
test('a', () => {});
test('b', () => {});";
        let d = run_ts(src);
        assert!(d.is_empty());
    }
}
