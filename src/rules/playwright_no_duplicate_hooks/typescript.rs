//! playwright-no-duplicate-hooks — disallow duplicate setup/teardown hooks.

use crate::diagnostic::{Diagnostic, Severity};
use std::collections::HashMap;

const TEST_MARKERS: &[&str] = &[".test.", ".spec.", "__tests__", "_test.", ".e2e."];

fn is_test_file(path: &std::path::Path) -> bool {
    let s = path.to_string_lossy();
    TEST_MARKERS.iter().any(|m| s.contains(m))
}

const HOOKS: &[&str] = &["beforeAll", "beforeEach", "afterAll", "afterEach"];

/// Recursively find duplicate hooks within describe blocks.
fn check_block(
    node: tree_sitter::Node,
    source: &[u8],
    ctx: &crate::rules::backend::CheckCtx,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let mut hook_counts: HashMap<String, usize> = HashMap::new();
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        // If this child is a describe call, recurse into its callback body.
        if child.kind() == "expression_statement"
            && let Some(expr) = child.named_child(0) {
                if is_describe_call(expr, source) {
                    if let Some(args) = expr.child_by_field_name("arguments") {
                        let ac = args.named_child_count();
                        if ac > 0
                            && let Some(cb) = args.named_child(ac - 1)
                                && let Some(body) = cb.child_by_field_name("body") {
                                    check_block(body, source, ctx, diagnostics);
                                }
                    }
                    continue;
                }
                // Check if this is a hook call.
                if let Some(name) = get_hook_name(expr, source) {
                    let entry = hook_counts.entry(name.clone()).or_insert(0);
                    *entry += 1;
                    if *entry > 1 {
                        let pos = expr.start_position();
                        diagnostics.push(Diagnostic {
                            path: std::sync::Arc::clone(&ctx.path_arc),
                            line: pos.row + 1,
                            column: pos.column + 1,
                            rule_id: "playwright-no-duplicate-hooks".into(),
                            message: format!("Duplicate {name} in describe block."),
                            severity: Severity::Warning,
                            span: None,
                        });
                    }
                }
            }
    }
}

fn is_describe_call(node: tree_sitter::Node, source: &[u8]) -> bool {
    if node.kind() != "call_expression" {
        return false;
    }
    let Some(callee) = node.child_by_field_name("function") else { return false };
    match callee.kind() {
        "identifier" => callee.utf8_text(source).unwrap_or("") == "describe",
        "member_expression" => {
            let Some(obj) = callee.child_by_field_name("object") else { return false };
            obj.utf8_text(source).unwrap_or("") == "describe"
        }
        _ => false,
    }
}

fn get_hook_name(node: tree_sitter::Node, source: &[u8]) -> Option<String> {
    if node.kind() != "call_expression" {
        return None;
    }
    let callee = node.child_by_field_name("function")?;
    let name = match callee.kind() {
        "identifier" => callee.utf8_text(source).unwrap_or(""),
        "member_expression" => {
            callee.child_by_field_name("property")?.utf8_text(source).unwrap_or("")
        }
        _ => return None,
    };
    if HOOKS.contains(&name) {
        Some(name.to_string())
    } else {
        None
    }
}

crate::ast_check! { on ["program"] => |node, source, ctx, diagnostics|
    if !is_test_file(ctx.path) {
        return;
    }

    // Only trigger on root program to do a single traversal.
    check_block(node, source, ctx, diagnostics);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::test_helpers::run_ts_with_path;

    fn run_ts(source: &str) -> Vec<Diagnostic> {
        run_ts_with_path(source, &Check, "app.test.ts")
    }

    #[test]
    fn flags_duplicate_before_each() {
        let src = "\
describe('suite', () => {
  beforeEach(() => {});
  beforeEach(() => {});
  test('a', () => {});
});";
        let d = run_ts(src);
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "playwright-no-duplicate-hooks");
    }

    #[test]
    fn allows_different_hooks() {
        let src = "\
describe('suite', () => {
  beforeEach(() => {});
  afterEach(() => {});
  test('a', () => {});
});";
        let d = run_ts(src);
        assert!(d.is_empty());
    }
}
