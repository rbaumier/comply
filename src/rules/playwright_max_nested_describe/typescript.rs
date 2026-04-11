//! playwright-max-nested-describe — limit nesting depth of describe blocks.

use crate::diagnostic::{Diagnostic, Severity};

const TEST_MARKERS: &[&str] = &[".test.", ".spec.", "__tests__", "_test.", ".e2e."];

fn is_test_file(path: &std::path::Path) -> bool {
    let s = path.to_string_lossy();
    TEST_MARKERS.iter().any(|m| s.contains(m))
}

const MAX_DEPTH: usize = 5;

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
                || callee.child_by_field_name("property")
                    .and_then(|p| p.utf8_text(source).ok())
                    .unwrap_or("") == "describe"
        }
        _ => false,
    }
}

fn check_describe_depth(
    node: tree_sitter::Node,
    source: &[u8],
    depth: usize,
    ctx: &crate::rules::backend::CheckCtx,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if is_describe_call(node, source) {
        let new_depth = depth + 1;
        if new_depth > MAX_DEPTH {
            let pos = node.start_position();
            diagnostics.push(Diagnostic {
                path: ctx.path.to_path_buf(),
                line: pos.row + 1,
                column: pos.column + 1,
                rule_id: "playwright-max-nested-describe".into(),
                message: format!(
                    "Describe depth {new_depth} exceeds maximum allowed {MAX_DEPTH}."
                ),
                severity: Severity::Warning,
            });
        }
        // Walk children at increased depth
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            check_describe_depth(child, source, new_depth, ctx, diagnostics);
        }
    } else {
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            check_describe_depth(child, source, depth, ctx, diagnostics);
        }
    }
}

crate::ast_check! { |node, source, ctx, diagnostics|
    if !is_test_file(ctx.path) {
        return;
    }

    // Only trigger on the root program node to avoid double counting.
    if node.kind() != "program" {
        return;
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        check_describe_depth(child, source, 0, ctx, diagnostics);
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
    fn flags_deeply_nested_describe() {
        let src = "\
describe('1', () => {
  describe('2', () => {
    describe('3', () => {
      describe('4', () => {
        describe('5', () => {
          describe('6', () => {
            test('deep', () => {});
          });
        });
      });
    });
  });
});";
        let d = run_ts(src);
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "playwright-max-nested-describe");
    }

    #[test]
    fn allows_five_levels() {
        let src = "\
describe('1', () => {
  describe('2', () => {
    describe('3', () => {
      describe('4', () => {
        describe('5', () => {
          test('ok', () => {});
        });
      });
    });
  });
});";
        let d = run_ts(src);
        assert!(d.is_empty());
    }
}
