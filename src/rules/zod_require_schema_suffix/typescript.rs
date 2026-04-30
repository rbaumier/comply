//! zod-require-schema-suffix backend.
//!
//! Walk every `variable_declarator` that lives inside an
//! `export_statement`. If its initializer is a call chain whose outer
//! base is `z.<something>(...)`, the binding's name must end in
//! `Schema`. This keeps schemas visually distinct from the inferred
//! types a consumer would write as `type Foo = z.infer<typeof FooSchema>`.

use crate::diagnostic::{Diagnostic, Severity};

/// True if `node` (or any ancestor) is an `export_statement`.
fn is_exported(node: tree_sitter::Node) -> bool {
    let mut current = node.parent();
    while let Some(n) = current {
        if n.kind() == "export_statement" {
            return true;
        }
        current = n.parent();
    }
    false
}

/// Unwrap a chain of `call_expression` / `member_expression` nodes to
/// reach the left-most expression. For `z.object({...}).strict()` the
/// chain resolves to the `member_expression` `z.object`.
fn chain_root<'a>(mut node: tree_sitter::Node<'a>) -> tree_sitter::Node<'a> {
    loop {
        match node.kind() {
            "call_expression" => {
                let Some(function) = node.child_by_field_name("function") else {
                    return node;
                };
                node = function;
            }
            "member_expression" => {
                let Some(object) = node.child_by_field_name("object") else {
                    return node;
                };
                // Stop when the object is a bare identifier — the
                // member_expression itself is the root (e.g. `z.object`).
                if object.kind() == "identifier" {
                    return node;
                }
                node = object;
            }
            _ => return node,
        }
    }
}

/// Whether `node` is a call chain that starts with `z.<anything>`.
fn starts_with_z(node: tree_sitter::Node, source: &[u8]) -> bool {
    let root = chain_root(node);
    if root.kind() != "member_expression" {
        return false;
    }
    let Some(object) = root.child_by_field_name("object") else {
        return false;
    };
    object.utf8_text(source).is_ok_and(|t| t == "z")
}

crate::ast_check! { on ["variable_declarator"] => |node, source, ctx, diagnostics|
    if !is_exported(node) {
        return;
    }
    let Some(name_node) = node.child_by_field_name("name") else {
        return;
    };
    // Skip destructuring patterns — we only care about simple identifiers.
    if name_node.kind() != "identifier" {
        return;
    }
    let Ok(name) = name_node.utf8_text(source) else {
        return;
    };
    if name.ends_with("Schema") {
        return;
    }
    let Some(value) = node.child_by_field_name("value") else {
        return;
    };
    if !starts_with_z(value, source) {
        return;
    }
    let pos = name_node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "zod-require-schema-suffix".into(),
        message: format!(
            "Exported Zod schema `{name}` should be renamed `{name}Schema` — \
             the suffix keeps the schema distinguishable from the inferred \
             TypeScript type."
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
    fn flags_export_without_schema_suffix() {
        assert_eq!(
            run_on("export const User = z.object({ id: z.string() });").len(),
            1
        );
    }

    #[test]
    fn flags_export_of_z_string() {
        assert_eq!(run_on("export const Email = z.string().email();").len(), 1);
    }

    #[test]
    fn allows_export_with_schema_suffix() {
        assert!(run_on("export const UserSchema = z.object({ id: z.string() });").is_empty());
    }

    #[test]
    fn allows_non_exported_declaration() {
        assert!(run_on("const User = z.object({ id: z.string() });").is_empty());
    }

    #[test]
    fn allows_export_that_is_not_zod() {
        assert!(run_on("export const User = { id: 1 };").is_empty());
    }
}
