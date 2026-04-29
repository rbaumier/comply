//! Flag `InferSelectModel<...>` / `InferInsertModel<...>` type references.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["generic_type"] prefilter = ["InferSelectModel", "InferInsertModel", "inferSelect", "inferInsert"] => |node, source, ctx, diagnostics|
    // A `generic_type` in tree-sitter-typescript represents `Foo<T>`.
    let Some(name_node) = node.child_by_field_name("name") else {
        // Fall back to scanning children for type_identifier.
        let mut cursor = node.walk();
        let mut found: Option<tree_sitter::Node<'_>> = None;
        for child in node.children(&mut cursor) {
            if child.kind() == "type_identifier" {
                found = Some(child);
                break;
            }
        }
        let Some(name_node) = found else { return };
        let name = name_node.utf8_text(source).unwrap_or("");
        if name == "InferSelectModel" || name == "InferInsertModel" {
            diagnostics.push(Diagnostic::at_node(
                ctx.path,
                &node,
                super::META.id,
                format!(
                    "Use `typeof table.${}` instead of `{}<typeof table>`.",
                    if name == "InferSelectModel" { "inferSelect" } else { "inferInsert" },
                    name
                ),
                Severity::Warning,
            ));
        }
        return;
    };
    let name = name_node.utf8_text(source).unwrap_or("");
    if name == "InferSelectModel" || name == "InferInsertModel" {
        diagnostics.push(Diagnostic::at_node(
            ctx.path,
            &node,
            super::META.id,
            format!(
                "Use `typeof table.${}` instead of `{}<typeof table>`.",
                if name == "InferSelectModel" { "inferSelect" } else { "inferInsert" },
                name
            ),
            Severity::Warning,
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(s, &Check)
    }

    #[test]
    fn flags_infer_select_model() {
        assert_eq!(run("type User = InferSelectModel<typeof users>").len(), 1);
    }

    #[test]
    fn flags_infer_insert_model() {
        assert_eq!(run("type NewUser = InferInsertModel<typeof users>").len(), 1);
    }

    #[test]
    fn allows_infer_select_property() {
        assert!(run("type User = typeof users.$inferSelect").is_empty());
    }

    #[test]
    fn allows_unrelated_generic() {
        assert!(run("type X = Array<string>").is_empty());
    }
}
