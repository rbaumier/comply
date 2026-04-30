//! Flags `type X = A & B & ...` where every intersection member is a
//! type reference (named type / generic type). Intersections that include
//! literal object types, primitive types, or utility types like
//! `Omit<...>` are left alone because they have no direct interface
//! equivalent.

use crate::diagnostic::{Diagnostic, Severity};

fn is_named_type_ref(node: tree_sitter::Node) -> bool {
    matches!(
        node.kind(),
        "type_identifier" | "generic_type" | "nested_type_identifier"
    )
}

crate::ast_check! { on ["type_alias_declaration"] => |node, source, ctx, diagnostics|
    let _ = source;
    let Some(value) = node.child_by_field_name("value") else { return };
    if value.kind() != "intersection_type" {
        return;
    }

    let mut cursor = value.walk();
    let members: Vec<_> = value.named_children(&mut cursor).collect();
    if members.len() < 2 {
        return;
    }
    if !members.iter().all(|m| is_named_type_ref(*m)) {
        return;
    }

    let Some(name_node) = node.child_by_field_name("name") else { return };
    let name = std::str::from_utf8(&source[name_node.byte_range()]).unwrap_or("");

    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        format!("Prefer `interface {name} extends ...` over `type {name} = A & B` for object composition."),
        Severity::Warning,
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(s, &Check)
    }

    #[test]
    fn flags_intersection_of_named_types() {
        let diags = run("type X = A & B;");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn flags_intersection_of_generic_types() {
        let diags = run("type X = Base<T> & Mixin;");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn allows_intersection_with_object_literal() {
        assert!(run("type X = A & { extra: string };").is_empty());
    }

    #[test]
    fn allows_plain_type_alias() {
        assert!(run("type X = string;").is_empty());
    }
}
