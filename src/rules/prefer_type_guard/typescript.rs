//! prefer-type-guard backend — `isX(): boolean` with typeof/instanceof should use type predicates.

use crate::diagnostic::{Diagnostic, Severity};

/// Check if a function body contains `typeof` or `instanceof` expressions.
fn body_has_type_check(body: tree_sitter::Node, source: &[u8]) -> bool {
    let cursor = body.walk();
    let mut stack = vec![body];
    while let Some(n) = stack.pop() {
        match n.kind() {
            "typeof" => return true,
            "instanceof_expression" => return true,
            "unary_expression" => {
                // `typeof x` is a unary_expression with operator "typeof"
                if let Some(op) = n.child_by_field_name("operator")
                    && op.utf8_text(source).ok() == Some("typeof")
                {
                    return true;
                }
            }
            "binary_expression" => {
                // Check left operand for typeof
                if let Some(left) = n.child_by_field_name("left") {
                    if left.kind() == "unary_expression"
                        && let Some(op) = left.child_by_field_name("operator")
                        && op.utf8_text(source).ok() == Some("typeof")
                    {
                        return true;
                    }
                    if left.kind() == "typeof_expression" {
                        return true;
                    }
                }
                // Check for instanceof
                if let Some(op) = n.child_by_field_name("operator")
                    && op.utf8_text(source).ok() == Some("instanceof")
                {
                    return true;
                }
            }
            _ => {}
        }
        let mut child_cursor = n.walk();
        for child in n.children(&mut child_cursor) {
            stack.push(child);
        }
    }
    let _ = cursor;
    false
}

crate::ast_check! { on ["function_declaration"] => |node, source, ctx, diagnostics|
    // Get function name — must start with "is" followed by uppercase
    let Some(name_node) = node.child_by_field_name("name") else { return };
    let Ok(name) = name_node.utf8_text(source) else { return };
    if !name.starts_with("is") {
        return;
    }
    // After "is" must be uppercase or digit
    let after_is = &name[2..];
    if after_is.is_empty() {
        return;
    }
    if !after_is.starts_with(|c: char| c.is_ascii_uppercase()) {
        return;
    }

    // Check return type annotation is `: boolean` (not a type predicate like `x is Type`)
    let Some(return_type) = node.child_by_field_name("return_type") else { return };
    let Ok(return_type_text) = return_type.utf8_text(source) else { return };
    // return_type includes the colon, e.g. ": boolean"
    let rt_trimmed = return_type_text.trim();
    let rt_inner = rt_trimmed.strip_prefix(':').unwrap_or(rt_trimmed).trim();
    if rt_inner != "boolean" {
        return;
    }

    // Check body for typeof / instanceof
    let Some(body) = node.child_by_field_name("body") else { return };
    if !body_has_type_check(body, source) {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "prefer-type-guard".into(),
        message: "Function `isX` returns `boolean` with type checks \u{2014} use a type predicate (`x is Type`) instead.".into(),
        severity: Severity::Warning,
        span: None,
    });
}


#[cfg(test)]
impl crate::rules::test_helpers::RunRule for Check {
    fn meta(&self) -> &'static crate::rules::meta::RuleMeta {
        &super::META
    }
    fn execute_with_ctx(
        &self,
        src: &str,
        path: &std::path::Path,
        project: &crate::project::ProjectCtx,
        file: &crate::rules::file_ctx::FileCtx,
    ) -> Vec<crate::diagnostic::Diagnostic> {
        crate::rules::test_helpers::run_ast_check(self, src, path, project, file)
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    #[test]
    fn flags_is_function_with_typeof() {
        let src = r#"
function isString(x: unknown): boolean {
    return typeof x === "string";
}
"#;
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_is_function_with_instanceof() {
        let src = r#"
function isError(x: unknown): boolean {
    return x instanceof Error;
}
"#;
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_type_predicate() {
        let src = r#"
function isString(x: unknown): x is string {
    return typeof x === "string";
}
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_non_is_function() {
        let src = r#"
function checkValue(x: unknown): boolean {
    return typeof x === "string";
}
"#;
        assert!(run_on(src).is_empty());
    }
}
