//! no-thenable backend — flag objects/classes that define a `then` property.
//!
//! Objects with a `then` method are "thenables" — `await` and
//! `Promise.resolve()` unwrap them automatically, which is almost
//! never the intent and causes subtle async bugs.

use crate::diagnostic::{Diagnostic, Severity};

/// Check whether a node's text is the identifier `then`.
fn is_then_name(node: tree_sitter::Node, source: &[u8]) -> bool {
    node.utf8_text(source).unwrap_or("") == "then"
}

crate::ast_check! { |node, source, ctx, diagnostics|
    match node.kind() {
        // Object literal property: `{ then() {} }` or `{ then: ... }`
        "pair" => {
            // Only match when inside an object (not destructuring).
            let parent = node.parent();
            if parent.is_none_or(|p| p.kind() != "object") {
                return;
            }
            let Some(key) = node.child_by_field_name("key") else { return };
            if key.kind() == "property_identifier" && is_then_name(key, source) {
                let pos = key.start_position();
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: pos.row + 1,
                    column: pos.column + 1,
                    rule_id: "no-thenable".into(),
                    message: "Do not add `then` to an object.".into(),
                    severity: Severity::Warning,
                    span: None,
                });
            }
        }
        // Object literal shorthand method: `{ then() {} }`
        "method_definition" => {
            // In an object literal, methods are also `method_definition` nodes.
            let parent = node.parent();

            // In a class body — flag `then` method/getter/setter.
            if parent.is_some_and(|p| p.kind() == "class_body") {
                let Some(name_node) = node.child_by_field_name("name") else { return };
                if is_then_name(name_node, source) {
                    let pos = name_node.start_position();
                    diagnostics.push(Diagnostic {
                        path: ctx.path.to_path_buf(),
                        line: pos.row + 1,
                        column: pos.column + 1,
                        rule_id: "no-thenable".into(),
                        message: "Do not add `then` to a class.".into(),
                        severity: Severity::Warning,
                        span: None,
                    });
                }
                return;
            }

            // In an object literal — shorthand methods live under `object`.
            if parent.is_none_or(|p| p.kind() != "object") {
                return;
            }
            let Some(name_node) = node.child_by_field_name("name") else { return };
            if is_then_name(name_node, source) {
                let pos = name_node.start_position();
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: pos.row + 1,
                    column: pos.column + 1,
                    rule_id: "no-thenable".into(),
                    message: "Do not add `then` to an object.".into(),
                    severity: Severity::Warning,
                    span: None,
                });
            }
        }
        // Class field: `class Foo { then = ... }` or `class Foo { static then = ... }`
        "public_field_definition" => {
            let parent = node.parent();
            if parent.is_none_or(|p| p.kind() != "class_body") {
                return;
            }
            let Some(name_node) = node.child_by_field_name("name") else { return };
            if is_then_name(name_node, source) {
                let pos = name_node.start_position();
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: pos.row + 1,
                    column: pos.column + 1,
                    rule_id: "no-thenable".into(),
                    message: "Do not add `then` to a class.".into(),
                    severity: Severity::Warning,
                    span: None,
                });
            }
        }
        // Export: `export function then() {}` / `export class then {}`
        "export_statement" => {
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                match child.kind() {
                    "function_declaration" | "class_declaration" => {
                        if let Some(name_node) = child.child_by_field_name("name")
                            && is_then_name(name_node, source) {
                                let pos = name_node.start_position();
                                diagnostics.push(Diagnostic {
                                    path: ctx.path.to_path_buf(),
                                    line: pos.row + 1,
                                    column: pos.column + 1,
                                    rule_id: "no-thenable".into(),
                                    message: "Do not export `then`.".into(),
                                    severity: Severity::Warning,
                                    span: None,
                                });
                            }
                    }
                    _ => {}
                }
            }
        }
        // Named export specifier: `export { foo as then }`
        "export_specifier" => {
            // The exported name is the `alias` field if present, otherwise `name`.
            let exported = node.child_by_field_name("alias")
                .or_else(|| node.child_by_field_name("name"));
            if let Some(exp) = exported
                && is_then_name(exp, source) {
                    let pos = exp.start_position();
                    diagnostics.push(Diagnostic {
                        path: ctx.path.to_path_buf(),
                        line: pos.row + 1,
                        column: pos.column + 1,
                        rule_id: "no-thenable".into(),
                        message: "Do not export `then`.".into(),
                        severity: Severity::Warning,
                        span: None,
                    });
                }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_object_with_then_method() {
        let d = run_on("const obj = { then() {} };");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("object"));
    }

    #[test]
    fn flags_object_with_then_property() {
        let d = run_on("const obj = { then: function() {} };");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("object"));
    }

    #[test]
    fn flags_class_with_then_method() {
        let d = run_on("class Foo { then() {} }");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("class"));
    }

    #[test]
    fn flags_class_with_then_field() {
        let d = run_on("class Foo { then = 42; }");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("class"));
    }

    #[test]
    fn flags_class_with_static_then() {
        let d = run_on("class Foo { static then() {} }");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_object_without_then() {
        assert!(run_on("const obj = { foo() {} };").is_empty());
    }

    #[test]
    fn allows_class_without_then() {
        assert!(run_on("class Foo { bar() {} }").is_empty());
    }

    #[test]
    fn flags_exported_function_then() {
        let d = run_on("export function then() {}");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("export"));
    }

    #[test]
    fn flags_export_specifier_then() {
        let d = run_on("const then = 1; export { then };");
        assert_eq!(d.len(), 1);
    }
}
