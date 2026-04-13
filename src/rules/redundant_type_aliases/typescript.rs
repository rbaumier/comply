//! redundant-type-aliases AST backend.
//!
//! Flags `type X = Y;` where Y is a single type identifier (no unions,
//! intersections, generics, etc.).

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "type_alias_declaration" {
        return;
    }

    // The RHS type. tree-sitter may not expose a "value" field on type_alias_declaration.
    // Try "value" first, then fall back to walking children for the type after '='.
    let value = node.child_by_field_name("value").or_else(|| {
        let mut cursor = node.walk();
        let mut found_eq = false;
        for child in node.children(&mut cursor) {
            if found_eq && child.kind() != ";" {
                return Some(child);
            }
            if child.kind() == "=" {
                found_eq = true;
            }
        }
        None
    });
    let Some(value) = value else { return };

    // Only flag if RHS is a single type_identifier or predefined_type
    // (plain name like `Foo` or primitive like `string`, no generics/union/intersection)
    if value.kind() != "type_identifier" && value.kind() != "predefined_type" {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "redundant-type-aliases".into(),
        message: "Type alias is just renaming \u{2014} use the original type directly or add structure.".into(),
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
    fn flags_simple_rename() {
        assert_eq!(run_on("type UserID = string;").len(), 1);
    }

    #[test]
    fn flags_identifier_rename() {
        assert_eq!(run_on("type Alias = OriginalType;").len(), 1);
    }

    #[test]
    fn allows_union_type() {
        assert!(run_on("type X = string | number;").is_empty());
    }

    #[test]
    fn allows_intersection_type() {
        assert!(run_on("type X = A & B;").is_empty());
    }

    #[test]
    fn allows_generic_type() {
        assert!(run_on("type X = Array<string>;").is_empty());
    }

    #[test]
    fn allows_object_type() {
        assert!(run_on("type X = { name: string };").is_empty());
    }
}
