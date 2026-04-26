//! inverted-assertion-arguments backend — flag `expect(literal).toBe(variable)`.

use crate::diagnostic::{Diagnostic, Severity};

/// Check if a node is a literal value (number, string, boolean, null, undefined).
fn is_literal_node(node: tree_sitter::Node) -> bool {
    matches!(
        node.kind(),
        "number" | "string" | "true" | "false" | "null" | "undefined"
    )
}

/// Check if a node is a simple variable/identifier (not a literal, not a call).
fn is_variable_node(node: tree_sitter::Node) -> bool {
    node.kind() == "identifier"
}

crate::ast_check! { on ["call_expression"] => |node, source, ctx, diagnostics|
    // Only check test files.
    let path_str = ctx.path.to_string_lossy();
    if !path_str.contains(".test.") && !path_str.contains(".spec.") && !path_str.contains("__tests__") && !path_str.contains("_test.") {
        return;
    }

    // Look for `.toBe(` or `.toEqual(` calls.
    let Some(callee) = node.child_by_field_name("function") else { return };
    if callee.kind() != "member_expression" {
        return;
    }
    let Some(method) = callee.child_by_field_name("property") else { return };
    let method_name = method.utf8_text(source).unwrap_or("");
    if method_name != "toBe" && method_name != "toEqual" {
        return;
    }

    // Get the matcher argument.
    let Some(args) = node.child_by_field_name("arguments") else { return };
    let mut arg_cursor = args.walk();
    let matcher_arg = args.children(&mut arg_cursor)
        .find(|c| c.kind() != "(" && c.kind() != ")" && c.kind() != ",");
    let Some(matcher_arg) = matcher_arg else { return };

    if !is_variable_node(matcher_arg) {
        return;
    }

    // Walk up to find the `expect(...)` call — the object of the member expression.
    let Some(object) = callee.child_by_field_name("object") else { return };

    // object should be a call_expression for `expect(...)`.
    if object.kind() != "call_expression" {
        return;
    }
    let Some(expect_fn) = object.child_by_field_name("function") else { return };
    let expect_name = expect_fn.utf8_text(source).unwrap_or("");
    if expect_name != "expect" {
        return;
    }

    // Get the expect argument.
    let Some(expect_args) = object.child_by_field_name("arguments") else { return };
    let mut expect_cursor = expect_args.walk();
    let expect_arg = expect_args.children(&mut expect_cursor)
        .find(|c| c.kind() != "(" && c.kind() != ")" && c.kind() != ",");
    let Some(expect_arg) = expect_arg else { return };

    if is_literal_node(expect_arg) {
        let pos = node.start_position();
        diagnostics.push(Diagnostic {
            path: ctx.path.to_path_buf(),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "inverted-assertion-arguments".into(),
            message: "Expected and actual are inverted — put the literal in `.toBe()`/`.toEqual()`, not in `expect()`.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    // Use a test file path for test context.
    fn run_test_file(source: &str) -> Vec<Diagnostic> {
        use std::path::Path;
        use crate::rules::backend::{AstCheck, CheckCtx};
        let mut parser = tree_sitter::Parser::new();
        parser.set_language(&tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into()).unwrap();
        let tree = parser.parse(source, None).unwrap();
        Check.check(&CheckCtx::for_test(Path::new("foo.test.ts"), source), &tree)
    }

    #[test]
    fn flags_literal_in_expect_variable_in_tobe() {
        assert_eq!(run_test_file(r#"expect(42).toBe(result);"#).len(), 1);
    }

    #[test]
    fn flags_string_literal_in_expect() {
        assert_eq!(run_test_file(r#"expect("hello").toEqual(name);"#).len(), 1);
    }

    #[test]
    fn allows_variable_in_expect_literal_in_tobe() {
        assert!(run_test_file(r#"expect(result).toBe(42);"#).is_empty());
    }

    #[test]
    fn allows_variable_in_both() {
        assert!(run_test_file(r#"expect(result).toBe(expected);"#).is_empty());
    }

    #[test]
    fn ignores_non_test_files() {
        // run_on uses "t.ts" (not a test file).
        assert!(run_on(r#"expect(42).toBe(result);"#).is_empty());
    }
}
