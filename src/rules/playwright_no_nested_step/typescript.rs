//! playwright-no-nested-step — disallow nested `test.step()` calls.

use crate::diagnostic::{Diagnostic, Severity};

const TEST_MARKERS: &[&str] = &[".test.", ".spec.", "__tests__", "_test.", ".e2e."];

fn is_test_file(path: &std::path::Path) -> bool {
    let s = path.to_string_lossy();
    TEST_MARKERS.iter().any(|m| s.contains(m))
}

fn is_step_call(node: tree_sitter::Node, source: &[u8]) -> bool {
    if node.kind() != "call_expression" {
        return false;
    }
    let Some(callee) = node.child_by_field_name("function") else {
        return false;
    };
    if callee.kind() != "member_expression" {
        return false;
    }
    let Some(obj) = callee.child_by_field_name("object") else {
        return false;
    };
    let Some(prop) = callee.child_by_field_name("property") else {
        return false;
    };
    obj.utf8_text(source).unwrap_or("") == "test" && prop.utf8_text(source).unwrap_or("") == "step"
}

/// Walk ancestors to check if we're inside a `test.step()` callback.
fn is_inside_step(node: tree_sitter::Node, source: &[u8]) -> bool {
    let mut cur = node.parent();
    while let Some(p) = cur {
        match p.kind() {
            "arrow_function" | "function_expression" | "function" => {
                // Check if this function's parent is a test.step() call.
                if let Some(pp) = p.parent()
                    && pp.kind() == "arguments"
                    && let Some(call) = pp.parent()
                    && is_step_call(call, source)
                {
                    return true;
                }
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

    if !is_step_call(node, source) {
        return;
    }

    if !is_inside_step(node, source) {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "playwright-no-nested-step".into(),
        message: "Do not nest `test.step()` methods.".into(),
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
        run_ts_with_path(&format!("{PW_IMPORT}{source}"), &Check, "app.test.ts")
    }

    #[test]
    fn flags_nested_step() {
        let src = r#"test.step('outer', async () => {
    await test.step('inner', async () => {});
});"#;
        let d = run_ts(src);
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "playwright-no-nested-step");
    }

    #[test]
    fn allows_flat_steps() {
        let src = r#"test.step('a', async () => {});
test.step('b', async () => {});"#;
        let d = run_ts(src);
        assert!(d.is_empty());
    }
}
