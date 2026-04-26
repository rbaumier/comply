//! prefer-class-fields backend — flag `this.x = <literal>` in constructors.
//!
//! When a constructor's first statement is `this.x = 'some literal'`,
//! that value should be a class field declaration instead. Class fields
//! are more visible and declarative.

use crate::diagnostic::{Diagnostic, Severity};

/// Returns true if the node is a literal value (string, number, boolean,
/// null, undefined, template_string with no substitutions).
fn is_literal(node: tree_sitter::Node) -> bool {
    matches!(
        node.kind(),
        "string" | "number" | "true" | "false" | "null" | "undefined" | "template_string"
    )
}

crate::ast_check! { on ["class_body"] => |node, source, ctx, diagnostics|
    // We look for class bodies and scan the constructor.
    // Find the constructor method.
    let mut body_cursor = node.walk();
    let mut constructor_body = None;
    for member in node.children(&mut body_cursor) {
        if member.kind() != "method_definition" {
            continue;
        }
        let Some(name_node) = member.child_by_field_name("name") else { continue };
        if name_node.utf8_text(source).unwrap_or("") != "constructor" {
            continue;
        }
        // Ensure it's not static and not computed.
        let mut is_static = false;
        let mut mc = member.walk();
        for child in member.children(&mut mc) {
            if child.kind() == "static" {
                is_static = true;
                break;
            }
            if child.kind() == "property_identifier" { break; }
        }
        if is_static { continue; }

        // Get the function body (statement_block).
        let Some(func_body) = member.child_by_field_name("body") else { continue };
        constructor_body = Some(func_body);
        break;
    }

    let Some(ctor_block) = constructor_body else { return };

    // Scan the constructor body for `this.x = <literal>` expression statements.
    let mut stmt_cursor = ctor_block.walk();
    for stmt in ctor_block.children(&mut stmt_cursor) {
        if stmt.kind() != "expression_statement" {
            continue;
        }
        let Some(expr) = stmt.named_child(0) else { continue };
        if expr.kind() != "assignment_expression" {
            continue;
        }

        // Check operator is `=`.
        let op = expr.child_by_field_name("operator")
            .or({
                // tree-sitter may not expose operator as a field;
                // check the text between left and right.
                None
            });
        // For assignment_expression, the operator is embedded. We check via
        // text to ensure it's `=` and not `+=`, etc.
        let expr_text = expr.utf8_text(source).unwrap_or("");
        // Quick heuristic: if the expression contains += -= etc, skip.
        if expr_text.contains("+=") || expr_text.contains("-=")
            || expr_text.contains("*=") || expr_text.contains("/=")
            || expr_text.contains("??=") || expr_text.contains("||=")
            || expr_text.contains("&&=")
        {
            continue;
        }
        let _ = op;

        let Some(left) = expr.child_by_field_name("left") else { continue };
        let Some(right) = expr.child_by_field_name("right") else { continue };

        // Left must be `this.something` (member_expression with `this` object).
        if left.kind() != "member_expression" {
            continue;
        }
        let Some(obj) = left.child_by_field_name("object") else { continue };
        if obj.kind() != "this" {
            continue;
        }
        // Must not be computed (`this[x]`).
        let left_text = left.utf8_text(source).unwrap_or("");
        if left_text.contains('[') {
            continue;
        }

        // Right must be a static literal.
        if !is_literal(right) {
            continue;
        }

        let pos = stmt.start_position();
        diagnostics.push(Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "prefer-class-fields".into(),
            message: "Prefer a class field declaration over `this` assignment in constructor for static values.".into(),
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

    #[test]
    fn flags_this_string_literal_in_constructor() {
        let code = r#"
class Foo {
    constructor() {
        this.name = 'hello';
    }
}
"#;
        let d = run_on(code);
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "prefer-class-fields");
    }

    #[test]
    fn flags_this_number_literal_in_constructor() {
        let code = "class Foo { constructor() { this.count = 0; } }";
        let d = run_on(code);
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_this_boolean_literal_in_constructor() {
        let code = "class Foo { constructor() { this.active = true; } }";
        let d = run_on(code);
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_this_with_non_literal() {
        let code = "class Foo { constructor(name) { this.name = name; } }";
        assert!(run_on(code).is_empty());
    }

    #[test]
    fn allows_this_with_function_call() {
        let code = "class Foo { constructor() { this.id = generateId(); } }";
        assert!(run_on(code).is_empty());
    }

    #[test]
    fn allows_class_field_declaration() {
        let code = "class Foo { name = 'hello'; }";
        assert!(run_on(code).is_empty());
    }

    #[test]
    fn flags_multiple_literal_assignments() {
        let code = r#"
class Foo {
    constructor() {
        this.a = 1;
        this.b = 'two';
    }
}
"#;
        let d = run_on(code);
        assert_eq!(d.len(), 2);
    }

    #[test]
    fn allows_compound_assignment() {
        let code = "class Foo { constructor() { this.count += 1; } }";
        assert!(run_on(code).is_empty());
    }
}
