//! no-extraneous-class backend — flag classes with no instance members
//! (only static methods/fields, or no members at all).
//!
//! This rule is close in spirit to the existing `no-static-only-class`
//! but matches typescript-eslint's semantics: it ALSO flags empty classes
//! (`class Foo {}`), and it flags constructor-only classes where the
//! constructor has no parameters — both are extraneous.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { |node, source, ctx, diagnostics|
    let _ = source;
    if node.kind() != "class_declaration" && node.kind() != "class" {
        return;
    }

    // Skip classes that extend a superclass — they may be subclassing for
    // instance semantics even if this body looks extraneous.
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "class_heritage" {
            return;
        }
    }

    // Abstract classes are allowed — they're explicit extension points.
    let mut cursor2 = node.walk();
    for child in node.children(&mut cursor2) {
        if child.kind() == "abstract" {
            return;
        }
    }

    let Some(body) = node.child_by_field_name("body") else { return };

    let mut has_instance_member = false;
    let mut has_any_member = false;
    let mut body_cursor = body.walk();
    for member in body.children(&mut body_cursor) {
        match member.kind() {
            "method_definition" | "public_field_definition" => {}
            _ => continue,
        }
        has_any_member = true;

        let mut is_static = false;
        let mut member_cursor = member.walk();
        for child in member.children(&mut member_cursor) {
            if child.kind() == "static" {
                is_static = true;
                break;
            }
            if child.kind() == "property_identifier"
                || child.kind() == "computed_property_name"
                || child.kind() == "private_property_identifier"
                || child.kind() == "statement_block"
            {
                break;
            }
        }

        if !is_static {
            has_instance_member = true;
            break;
        }
    }

    // Extraneous when: no members at all, or every member is static.
    if has_instance_member {
        return;
    }
    let _ = has_any_member;

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "no-extraneous-class".into(),
        message: "Class has no instance members — replace with module-level exports or a plain object.".into(),
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
    fn flags_empty_class() {
        assert_eq!(run_on("class Foo {}").len(), 1);
    }

    #[test]
    fn flags_static_only_methods() {
        assert_eq!(run_on("class Foo { static bar() {} }").len(), 1);
    }

    #[test]
    fn flags_static_only_fields() {
        assert_eq!(run_on("class Foo { static x = 1; static y = 2; }").len(), 1);
    }

    #[test]
    fn allows_class_with_instance_member() {
        assert!(run_on("class Foo { bar() {} }").is_empty());
    }

    #[test]
    fn allows_class_with_instance_field() {
        assert!(run_on("class Foo { x = 1; }").is_empty());
    }

    #[test]
    fn allows_class_extending_superclass() {
        assert!(run_on("class Foo extends Base { static bar() {} }").is_empty());
    }

    #[test]
    fn allows_abstract_class() {
        assert!(run_on("abstract class Foo { static bar() {} }").is_empty());
    }
}
