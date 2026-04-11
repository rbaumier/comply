//! ts-no-duplicate-enum-values backend — walk `enum_declaration`, collect
//! member values, flag duplicates.
//!
//! Tree-sitter node structure:
//!   enum_declaration > enum_body > enum_assignment { name, value }

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "enum_declaration" {
        return;
    }
    let Some(body) = node.child_by_field_name("body") else {
        return;
    };
    let mut seen: Vec<String> = Vec::new();
    let mut cursor = body.walk();
    for child in body.named_children(&mut cursor) {
        if child.kind() != "enum_assignment" {
            continue;
        }
        let Some(value_node) = child.child_by_field_name("value") else {
            continue;
        };
        let value_text = &source[value_node.byte_range()];
        let Ok(val) = std::str::from_utf8(value_text) else {
            continue;
        };
        let val = val.trim();
        if val.is_empty() {
            continue;
        }
        if seen.contains(&val.to_string()) {
            let pos = value_node.start_position();
            diagnostics.push(Diagnostic {
                path: ctx.path.to_path_buf(),
                line: pos.row + 1,
                column: pos.column + 1,
                rule_id: "ts-no-duplicate-enum-values".into(),
                message: format!("Duplicate enum member value `{val}`."),
                severity: Severity::Warning,
            });
        } else {
            seen.push(val.to_string());
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
    fn flags_duplicate_number_values() {
        let diags = run_on("enum E { A = 1, B = 1 }");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("Duplicate"));
    }

    #[test]
    fn flags_duplicate_string_values() {
        let diags = run_on(r#"enum E { A = "x", B = "x" }"#);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn allows_unique_values() {
        assert!(run_on("enum E { A = 1, B = 2 }").is_empty());
    }

    #[test]
    fn allows_no_initializer() {
        assert!(run_on("enum E { A, B, C }").is_empty());
    }

    #[test]
    fn flags_multiple_duplicates() {
        let diags = run_on("enum E { A = 1, B = 1, C = 1 }");
        assert_eq!(diags.len(), 2);
    }
}
