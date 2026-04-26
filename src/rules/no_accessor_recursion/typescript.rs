//! no-accessor-recursion backend — flag getters/setters that recurse on `this`.
//!
//! `get foo() { return this.foo; }` triggers infinite recursion at runtime.
//! Same for `set foo(v) { this.foo = v; }`. The fix is to use a backing
//! field like `this._foo` or a `WeakMap`.

use crate::diagnostic::{Diagnostic, Severity};

/// Walk up from a node to find the closest `method_definition` ancestor
/// that is a getter or setter. Returns `(kind, property_name)`.
fn find_accessor_ancestor<'a>(
    node: tree_sitter::Node<'a>,
    source: &[u8],
) -> Option<(&'a str, String)> {
    let mut current = node.parent();
    while let Some(n) = current {
        if n.kind() == "method_definition" {
            // Check if it's a getter or setter by looking for "get"/"set" child tokens.
            let mut cursor = n.walk();
            let mut accessor_kind = None;
            for child in n.children(&mut cursor) {
                match child.kind() {
                    "get" => { accessor_kind = Some("get"); break; }
                    "set" => { accessor_kind = Some("set"); break; }
                    // If we hit the name or body first, it's a regular method.
                    "property_identifier" | "private_property_identifier"
                    | "statement_block" | "formal_parameters" => break,
                    _ => {}
                }
            }
            if let Some(kind) = accessor_kind
                && let Some(name_node) = n.child_by_field_name("name") {
                    let name = name_node.utf8_text(source).unwrap_or("").to_string();
                    return Some((kind, name));
                }
            // It's a method but not get/set — stop searching.
            return None;
        }
        // Don't cross class boundaries or non-arrow function boundaries.
        if n.kind() == "class_body" || n.kind() == "class_declaration" || n.kind() == "class" {
            return None;
        }
        // Arrow functions inherit `this` so we traverse through them.
        // Regular functions define their own `this` so stop.
        if n.kind() == "function_declaration" || n.kind() == "function" || n.kind() == "generator_function" {
            return None;
        }
        // function_expression (non-arrow) also defines its own `this`.
        if n.kind() == "function_expression" || n.kind() == "generator_function" {
            return None;
        }
        current = n.parent();
    }
    None
}

crate::ast_check! { on ["this"] => |node, source, ctx, diagnostics|
    let Some(parent) = node.parent() else { return };

    // We need `this` to be part of a member_expression: `this.foo`
    if parent.kind() != "member_expression" {
        return;
    }

    // `this` must be the object, not the property.
    let Some(obj) = parent.child_by_field_name("object") else { return };
    if obj.id() != node.id() {
        return;
    }

    // Must not be computed access (this[foo]).
    let Some(prop) = parent.child_by_field_name("property") else { return };
    if prop.kind() != "property_identifier" && prop.kind() != "private_property_identifier" {
        return;
    }

    let prop_name = prop.utf8_text(source).unwrap_or("");

    // Find the enclosing getter/setter.
    let Some((accessor_kind, accessor_name)) = find_accessor_ancestor(node, source) else { return };

    // The property being accessed must match the accessor name.
    if prop_name != accessor_name {
        return;
    }

    // For getters: flag reads of `this.foo` (i.e., not on the left side of assignment).
    // For setters: flag writes to `this.foo` (i.e., on the left side of assignment).
    if accessor_kind == "get" {
        // A getter reading its own property is recursion — unless it's
        // being written to (which would be unusual in a getter but valid).
        let grandparent = parent.parent();
        let is_write_target = grandparent.is_some_and(|gp| {
            (gp.kind() == "assignment_expression" || gp.kind() == "augmented_assignment_expression")
                && gp.child_by_field_name("left").is_some_and(|l| l.id() == parent.id())
        });
        if !is_write_target {
            let pos = parent.start_position();
            diagnostics.push(Diagnostic {
                path: std::sync::Arc::clone(&ctx.path_arc),
                line: pos.row + 1,
                column: pos.column + 1,
                rule_id: "no-accessor-recursion".into(),
                message: "Recursive access to `this` within getter causes infinite recursion.".into(),
                severity: Severity::Warning,
                span: None,
            });
        }
    } else if accessor_kind == "set" {
        // A setter writing to its own property is recursion.
        let grandparent = parent.parent();
        let is_write_target = grandparent.is_some_and(|gp| {
            match gp.kind() {
                "assignment_expression" | "augmented_assignment_expression" => {
                    gp.child_by_field_name("left").is_some_and(|l| l.id() == parent.id())
                }
                "update_expression" => {
                    gp.child_by_field_name("argument").is_some_and(|a| a.id() == parent.id())
                }
                _ => false,
            }
        });
        if is_write_target {
            let pos = parent.start_position();
            diagnostics.push(Diagnostic {
                path: std::sync::Arc::clone(&ctx.path_arc),
                line: pos.row + 1,
                column: pos.column + 1,
                rule_id: "no-accessor-recursion".into(),
                message: "Recursive access to `this` within setter causes infinite recursion.".into(),
                severity: Severity::Warning,
                span: None,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_getter_reading_own_property() {
        let code = r#"
class Foo {
    get bar() { return this.bar; }
}
"#;
        let d = run_on(code);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("getter"));
    }

    #[test]
    fn flags_setter_writing_own_property() {
        let code = r#"
class Foo {
    set bar(value) { this.bar = value; }
}
"#;
        let d = run_on(code);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("setter"));
    }

    #[test]
    fn allows_getter_reading_different_property() {
        let code = r#"
class Foo {
    get bar() { return this._bar; }
}
"#;
        assert!(run_on(code).is_empty());
    }

    #[test]
    fn allows_setter_writing_different_property() {
        let code = r#"
class Foo {
    set bar(value) { this._bar = value; }
}
"#;
        assert!(run_on(code).is_empty());
    }

    #[test]
    fn allows_regular_method() {
        let code = r#"
class Foo {
    bar() { return this.bar; }
}
"#;
        // Regular methods can reference themselves (e.g., recursion is intentional).
        // This rule only targets get/set accessors.
        assert!(run_on(code).is_empty());
    }

    #[test]
    fn flags_object_literal_getter() {
        let code = r#"
const obj = {
    get foo() { return this.foo; }
};
"#;
        let d = run_on(code);
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_object_literal_setter() {
        let code = r#"
const obj = {
    set foo(v) { this.foo = v; }
};
"#;
        let d = run_on(code);
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_getter_with_arrow_function_using_this() {
        // Arrow inside getter — `this` still refers to the enclosing class.
        let code = r#"
class Foo {
    get bar() {
        const fn = () => this.bar;
        return fn();
    }
}
"#;
        let d = run_on(code);
        // The arrow function inherits `this`, so `this.bar` inside
        // an arrow inside `get bar()` is still recursive.
        assert_eq!(d.len(), 1);
    }
}
