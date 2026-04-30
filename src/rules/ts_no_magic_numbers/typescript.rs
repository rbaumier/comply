//! no-magic-numbers TS backend — flag numeric literals that are not in
//! an allowed context (const declarations, enums, type annotations,
//! default parameter values, array indices 0/1/-1).
//!
//! TS-specific: also allows numbers in `readonly` class properties,
//! enum members, and numeric literal types.

use crate::diagnostic::{Diagnostic, Severity};

/// Numeric values so idiomatic that flagging them is pure noise:
/// `-1` (not-found sentinel), `0` (index / false / identity), `1`
/// (increment / true / first element). Any other value — including
/// `2` — must still be extracted into a named constant. Numbers
/// embedded in string literals (e.g. Tailwind classes like `"p-4"`)
/// are already ignored because this check only visits `number` AST
/// nodes, never string contents.
const ALLOWED: &[&str] = &["-1", "0", "1", "0.0", "1.0"];

fn is_allowed_context(node: tree_sitter::Node) -> bool {
    let mut current = node.parent();
    while let Some(parent) = current {
        match parent.kind() {
            // const declaration initializer — named constant.
            "variable_declarator" => {
                if let Some(gp) = parent.parent() {
                    let gpk = gp.kind();
                    if gpk == "lexical_declaration" || gpk == "variable_declaration" {
                        // Check if it's a `const`.
                        let mut cursor = gp.walk();
                        for child in gp.children(&mut cursor) {
                            if child.kind() == "const" {
                                return true;
                            }
                        }
                        // Also check text prefix.
                        // Fallthrough — not const.
                    }
                }
            }
            // Enum member value.
            "enum_assignment" | "enum_member" | "enum_body" => return true,
            // Type annotation / type literal.
            "type_annotation" | "literal_type" => return true,
            // Default parameter value.
            "required_parameter" | "optional_parameter" => return true,
            // Readonly class property.
            "public_field_definition" | "property_definition" => return true,
            // Array index access.
            "subscript_expression" => {
                // Check if this number is the index (second child).
                if let Some(index) = parent.child_by_field_name("index")
                    && index.id() == node.id()
                {
                    return true;
                }
            }
            _ => {}
        }
        current = parent.parent();
    }
    false
}

crate::ast_check! { on ["number"] => |node, source, ctx, diagnostics|
    if ctx.file.path_segments.in_test_dir { return; }
    if ctx.path.to_string_lossy().contains("/examples/") { return; }
    let text = match std::str::from_utf8(&source[node.byte_range()]) {
        Ok(t) => t,
        Err(_) => return,
    };

    // Allow universally understood values.
    if ALLOWED.contains(&text) {
        return;
    }

    // Check for unary minus: parent is unary_expression with "-".
    if let Some(parent) = node.parent()
        && parent.kind() == "unary_expression" {
            let parent_text = std::str::from_utf8(&source[parent.byte_range()]).unwrap_or("");
            if ALLOWED.contains(&parent_text) {
                return;
            }
        }

    if is_allowed_context(node) {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: super::META.id.into(),
        message: format!(
            "Magic number `{text}` — extract into a named constant."
        ),
        severity: Severity::Warning,
        span: None,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_magic_number() {
        let diags = run_on("const timeout = getTimeout(); if (timeout > 3000) {}");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("3000"));
    }

    #[test]
    fn allows_const_declaration() {
        assert!(run_on("const MAX_TIMEOUT = 3000;").is_empty());
    }

    #[test]
    fn allows_zero_and_one() {
        assert!(run_on("const arr = items[0]; const len = arr.length - 1;").is_empty());
    }

    #[test]
    fn allows_enum_values() {
        assert!(run_on("enum Status { Active = 200, Error = 500 }").is_empty());
    }

    // Allowlist covers the three universally-idiomatic literals.
    #[test]
    fn allowlist_zero() {
        assert!(run_on("function f(x) { return x + 0; }").is_empty());
    }

    #[test]
    fn allowlist_one() {
        assert!(run_on("function f(x) { return x + 1; }").is_empty());
    }

    #[test]
    fn allowlist_minus_one_unary() {
        // `-1` parses as unary_expression(- , number(1)) — the walker
        // must recognise the wrapped form, not just a literal token.
        assert!(run_on("function f(x) { return x + -1; }").is_empty());
    }

    // Anything outside the allowlist is still a magic number.
    #[test]
    fn flags_bare_forty_two() {
        let diags = run_on("function f(x) { return x + 42; }");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("42"));
    }

    #[test]
    fn flags_negative_forty_two() {
        let diags = run_on("function f(x) { return x + -42; }");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("42"));
    }

    #[test]
    fn flags_two_now_that_it_is_out_of_allowlist() {
        // `2` was previously allowlisted; narrowed to {-1, 0, 1}
        // because halving/doubling factors are still meaningful
        // constants worth naming.
        let diags = run_on("function f(x) { return x + 2; }");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("2"));
    }

    #[test]
    fn ignores_numbers_inside_string_literals() {
        // Tailwind utilities like `"p-4"` are CSS class names, not
        // programmatic magic numbers. The AST only exposes a `string`
        // node here — no `number` child is walked, so nothing to skip.
        assert!(run_on(r#"function f() { return "p-4 mb-6 h-2.5"; }"#).is_empty());
    }

    #[test]
    fn flags_both_operands_of_arithmetic() {
        let diags = run_on("function f(x) { return x + 5 * 60; }");
        assert_eq!(diags.len(), 2);
    }
}
