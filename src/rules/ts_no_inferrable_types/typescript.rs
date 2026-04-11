//! ts-no-inferrable-types backend — flag variable declarations where the
//! type annotation is trivially inferred from a literal initializer.
//!
//! Detection: walk `variable_declarator` nodes that have both a
//! `type_annotation` and a literal `value` whose type matches.

use crate::diagnostic::{Diagnostic, Severity};

/// Map from type annotation text to the literal node kinds it's inferred from.
fn is_inferrable(annotation: &str, value_kind: &str) -> Option<&'static str> {
    match (annotation, value_kind) {
        ("number", "number") => Some("number"),
        ("string", "string") => Some("string"),
        ("string", "template_string") => Some("string"),
        ("boolean", "true") => Some("boolean"),
        ("boolean", "false") => Some("boolean"),
        ("null", "null") => Some("null"),
        ("undefined", "undefined") => Some("undefined"),
        _ => None,
    }
}

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "variable_declarator" {
        return;
    }
    // Must have both type annotation and value
    let Some(type_ann) = node.child_by_field_name("type") else {
        return;
    };
    let Some(value) = node.child_by_field_name("value") else {
        return;
    };

    // Extract the type name from the type_annotation node
    // type_annotation has structure: `: <type_node>`
    let mut type_cursor = type_ann.walk();
    let type_node = type_ann.named_children(&mut type_cursor)
        .next();
    let Some(type_node) = type_node else {
        return;
    };

    // Only care about simple predefined types
    let type_kind = type_node.kind();
    if type_kind != "predefined_type" && type_kind != "literal_type" {
        return;
    }

    let type_text = &source[type_node.byte_range()];
    let Ok(type_str) = std::str::from_utf8(type_text) else {
        return;
    };

    let value_kind = value.kind();
    if let Some(type_name) = is_inferrable(type_str, value_kind) {
        let pos = type_ann.start_position();
        diagnostics.push(Diagnostic {
            path: ctx.path.to_path_buf(),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "ts-no-inferrable-types".into(),
            message: format!(
                "Type `{type_name}` is trivially inferred from the literal — \
                 remove the type annotation."
            ),
            severity: Severity::Warning,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_number_literal() {
        let diags = run_on("const x: number = 5;");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("`number`"));
    }

    #[test]
    fn flags_string_literal() {
        let diags = run_on(r#"const s: string = "hello";"#);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn flags_boolean_literal() {
        let diags = run_on("const b: boolean = true;");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn allows_non_literal_init() {
        assert!(run_on("const x: number = getValue();").is_empty());
    }

    #[test]
    fn allows_different_type_and_value() {
        assert!(run_on("const x: string | undefined = getValue();").is_empty());
    }
}
