//! ts-explicit-member-accessibility backend — flag class members (methods
//! and properties) that lack an `accessibility_modifier` child.
//!
//! In tree-sitter-typescript, class bodies contain `method_definition` and
//! `public_field_definition` children. An explicit accessibility is
//! expressed as an `accessibility_modifier` child (values: `public`,
//! `private`, `protected`). TypeScript's `#private` syntax is a different
//! node — it is already explicitly private, so we don't flag it here.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["method_definition", "public_field_definition"] => |node, source, ctx, diagnostics|
    // Only flag members directly inside a class body.
    let Some(parent) = node.parent() else { return };
    if parent.kind() != "class_body" {
        return;
    }

    if has_accessibility_modifier(node) {
        return;
    }

    // `#name` private identifiers are already explicitly private.
    if has_private_identifier_name(node) {
        return;
    }

    let name = node
        .child_by_field_name("name")
        .and_then(|n| n.utf8_text(source).ok())
        .unwrap_or("<member>");
    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "ts-explicit-member-accessibility".into(),
        message: format!(
            "Class member '{name}' is missing an accessibility modifier. \
             Add `public`, `private`, or `protected`."
        ),
        severity: Severity::Warning,
        span: None,
    });
}

fn has_accessibility_modifier(node: tree_sitter::Node) -> bool {
    let mut cursor = node.walk();
    node.children(&mut cursor)
        .any(|c| c.kind() == "accessibility_modifier")
}

fn has_private_identifier_name(node: tree_sitter::Node) -> bool {
    node.child_by_field_name("name")
        .is_some_and(|n| n.kind() == "private_property_identifier")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_method_without_accessibility() {
        let diags = run_on("class A { foo() {} }");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("'foo'"));
    }

    #[test]
    fn flags_property_without_accessibility() {
        let diags = run_on("class A { name: string = 'a'; }");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn allows_public_method() {
        assert!(run_on("class A { public foo() {} }").is_empty());
    }

    #[test]
    fn allows_private_method() {
        assert!(run_on("class A { private foo() {} }").is_empty());
    }

    #[test]
    fn allows_protected_property() {
        assert!(run_on("class A { protected name: string = 'a'; }").is_empty());
    }

    #[test]
    fn allows_hash_private_method() {
        assert!(run_on("class A { #foo() {} }").is_empty());
    }
}
