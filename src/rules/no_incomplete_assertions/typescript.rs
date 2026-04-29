//! no-incomplete-assertions backend — flag `expect()` calls without
//! a matcher.

use crate::diagnostic::{Diagnostic, Severity};

const MATCHERS: &[&str] = &[
    "toBe", "toEqual", "toMatch", "toThrow", "toContain",
    "toBeTruthy", "toBeFalsy", "toBeNull", "toBeUndefined",
    "toBeDefined", "toBeGreaterThan", "toBeLessThan",
    "toBeInstanceOf", "toHaveBeenCalled", "toHaveBeenCalledWith",
    "toHaveLength", "toHaveProperty", "toMatchObject",
    "toMatchSnapshot", "toMatchInlineSnapshot", "toStrictEqual",
    "resolves", "rejects", "toBeCloseTo", "toBeNaN",
];

fn is_test_file(path: &std::path::Path) -> bool {
    let s = path.to_string_lossy();
    s.contains(".test.") || s.contains(".spec.") || s.contains("__tests__") || s.contains("_test.")
}

crate::ast_check! { on ["expression_statement"] prefilter = ["expect"] => |node, source, ctx, diagnostics|
    if !is_test_file(ctx.path) {
        return;
    }

    // Match expression_statement -> call_expression where the callee is `expect`.
    let Some(expr) = node.named_child(0) else { return };

    // Case 1: bare `expect(x);`
    if expr.kind() == "call_expression"
        && let Some(func) = expr.child_by_field_name("function")
            && func.kind() == "identifier"
                && let Ok(name) = func.utf8_text(source)
                    && name == "expect" {
                        let pos = node.start_position();
                        diagnostics.push(Diagnostic {
                            path: std::sync::Arc::clone(&ctx.path_arc),
                            line: pos.row + 1,
                            column: pos.column + 1,
                            rule_id: "no-incomplete-assertions".into(),
                            message: "Incomplete assertion — `expect()` without a matcher tests nothing.".into(),
                            severity: Severity::Error,
                            span: None,
                        });
                        return;
                    }

    // Case 2: `expect(x).not;` — member_expression without a call
    if expr.kind() == "member_expression" {
        // Walk the chain to find if expect is the root and there's no matcher.
        if has_expect_root_without_matcher(expr, source) {
            let pos = node.start_position();
            diagnostics.push(Diagnostic {
                path: std::sync::Arc::clone(&ctx.path_arc),
                line: pos.row + 1,
                column: pos.column + 1,
                rule_id: "no-incomplete-assertions".into(),
                message: "Incomplete assertion — `expect()` without a matcher tests nothing.".into(),
                severity: Severity::Error,
                span: None,
            });
        }
    }
}

fn has_expect_root_without_matcher(node: tree_sitter::Node, source: &[u8]) -> bool {
    // Check if property is a known matcher (it shouldn't be, for this to be incomplete).
    if let Some(prop) = node.child_by_field_name("property")
        && let Ok(prop_name) = prop.utf8_text(source)
            && MATCHERS.contains(&prop_name) {
                return false;
            }

    // Check if the object is `expect(...)` call.
    if let Some(obj) = node.child_by_field_name("object")
        && obj.kind() == "call_expression"
            && let Some(func) = obj.child_by_field_name("function")
                && func.kind() == "identifier"
                    && let Ok(name) = func.utf8_text(source) {
                        return name == "expect";
                    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::backend::AstCheck;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        // Use a test file path so the check runs.
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into())
            .unwrap();
        let tree = parser.parse(source, None).unwrap();
        let ctx = crate::rules::backend::CheckCtx::for_test(
            std::path::Path::new("foo.test.ts"),
            source,
        );
        Check.check(&ctx, &tree)
    }

    #[test]
    fn flags_bare_expect() {
        assert_eq!(run_on("expect(value);").len(), 1);
    }

    #[test]
    fn flags_expect_dot_not() {
        assert_eq!(run_on("expect(value).not;").len(), 1);
    }

    #[test]
    fn allows_expect_with_tobe() {
        assert!(run_on("expect(value).toBe(true);").is_empty());
    }

    #[test]
    fn allows_expect_with_to_equal() {
        assert!(run_on("expect(value).toEqual({ a: 1 });").is_empty());
    }
}
