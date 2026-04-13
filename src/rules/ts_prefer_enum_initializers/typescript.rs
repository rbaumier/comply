//! ts-prefer-enum-initializers backend — flag enum members that lack an
//! explicit initializer.
//!
//! Detection: walk `enum_declaration` > `enum_body`, check each member
//! (property_identifier without `enum_assignment`) for a missing value.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "enum_declaration" {
        return;
    }
    let Some(body) = node.child_by_field_name("body") else {
        return;
    };
    let mut cursor = body.walk();
    for child in body.named_children(&mut cursor) {
        // In tree-sitter-typescript, enum members with an initializer are
        // `enum_assignment` nodes; members without one are
        // `property_identifier` nodes directly.
        if child.kind() == "property_identifier" {
            let name = std::str::from_utf8(&source[child.byte_range()])
                .unwrap_or("<unknown>");
            let pos = child.start_position();
            diagnostics.push(Diagnostic {
                path: ctx.path.to_path_buf(),
                line: pos.row + 1,
                column: pos.column + 1,
                rule_id: "ts-prefer-enum-initializers".into(),
                message: format!(
                    "The value of the member `{name}` should be explicitly defined."
                ),
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
    fn flags_uninitialized_members() {
        let diags = run_on("enum E { A, B, C }");
        assert_eq!(diags.len(), 3);
    }

    #[test]
    fn allows_all_initialized() {
        assert!(run_on("enum E { A = 1, B = 2, C = 3 }").is_empty());
    }

    #[test]
    fn flags_partially_initialized() {
        let diags = run_on("enum E { A = 1, B, C = 3 }");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("B"));
    }
}
