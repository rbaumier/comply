//! assertions-in-tests AST backend — test functions must contain at
//! least one assertion.

use crate::diagnostic::{Diagnostic, Severity};

fn is_test_file(path: &std::path::Path) -> bool {
    let s = path.to_string_lossy();
    s.contains(".test.") || s.contains(".spec.") || s.contains("__tests__") || s.contains("_test.")
}

/// Check whether a subtree contains an assertion call (expect, assert, .should,
/// .toBe, .toEqual, .toMatch, .toThrow).
fn has_assertion(node: tree_sitter::Node, source: &[u8]) -> bool {
    match node.kind() {
        "call_expression" => {
            let text = node.utf8_text(source).unwrap_or("");
            if text.contains("expect(") || text.contains("assert") {
                return true;
            }
        }
        "member_expression" => {
            let Some(prop) = node.child_by_field_name("property") else {
                return false;
            };
            let name = prop.utf8_text(source).unwrap_or("");
            if matches!(name, "should" | "toBe" | "toEqual" | "toMatch" | "toThrow") {
                return true;
            }
        }
        // Don't descend into nested test/it blocks.
        _ => {}
    }

    let count = node.child_count();
    for i in 0..count {
        if has_assertion(node.child(i).unwrap(), source) {
            return true;
        }
    }
    false
}

/// Extract the test name from the first string argument of a call.
fn extract_test_name(args: tree_sitter::Node, source: &[u8]) -> String {
    if let Some(first) = args.named_child(0)
        && matches!(first.kind(), "string" | "template_string") {
            let raw = first.utf8_text(source).unwrap_or("unnamed");
            // Strip quotes
            return raw
                .trim_start_matches(['\'', '"', '`'])
                .trim_end_matches(['\'', '"', '`'])
                .to_string();
        }
    "unnamed".to_string()
}

crate::ast_check! { on ["call_expression"] => |node, source, ctx, diagnostics|
    if !is_test_file(ctx.path) {
        return;
    }
    let Some(func) = node.child_by_field_name("function") else { return };
    if func.kind() != "identifier" {
        return;
    }
    let callee = func.utf8_text(source).unwrap_or("");
    if callee != "it" && callee != "test" {
        return;
    }

    let Some(args) = node.child_by_field_name("arguments") else { return };

    // The callback is typically the second argument.
    let Some(callback) = args.named_child(1) else { return };
    if !matches!(callback.kind(), "arrow_function" | "function" | "function_expression") {
        return;
    }

    let Some(body) = callback.child_by_field_name("body") else { return };

    if !has_assertion(body, source) {
        let name = extract_test_name(args, source);
        let pos = node.start_position();
        diagnostics.push(Diagnostic {
            path: ctx.path.to_path_buf(),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "assertions-in-tests".into(),
            message: format!("Test `{name}` has no assertion — add `expect(...)` or similar."),
            severity: Severity::Error,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        // Use a test-file path so the rule activates.
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into())
            .unwrap();
        let tree = parser.parse(source, None).unwrap();
        let ctx = crate::rules::backend::CheckCtx::for_test(
            std::path::Path::new("foo.test.ts"),
            source,
        );
        use crate::rules::backend::AstCheck;
        Check.check(&ctx, &tree)
    }

    fn run_non_test(source: &str) -> Vec<Diagnostic> {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into())
            .unwrap();
        let tree = parser.parse(source, None).unwrap();
        let ctx = crate::rules::backend::CheckCtx::for_test(
            std::path::Path::new("foo.ts"),
            source,
        );
        use crate::rules::backend::AstCheck;
        Check.check(&ctx, &tree)
    }

    #[test]
    fn flags_test_without_assertion() {
        let src = r#"
test("should work", () => {
  const x = 1;
});
"#;
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_test_with_expect() {
        let src = r#"
test("should work", () => {
  expect(1).toBe(1);
});
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_it_block_without_assertion() {
        let src = r#"
it("does something", () => {
  const result = doThing();
});
"#;
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_test_with_assert() {
        let src = r#"
test("works", () => {
  assert.equal(a, b);
});
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_non_test_files() {
        let src = r#"
test("should work", () => {
  const x = 1;
});
"#;
        assert!(run_non_test(src).is_empty());
    }
}
