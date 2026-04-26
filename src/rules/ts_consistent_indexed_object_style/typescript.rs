//! ts-consistent-indexed-object-style backend — default "record" mode:
//! flag type literals and interfaces with a single index signature, since
//! `Record<K, V>` is shorter and more readable.
//!
//! Tree-sitter structure:
//!   object_type / interface_body with exactly one child: index_signature

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { |node, source, ctx, diagnostics|
    let kind = node.kind();

    // Match both standalone object types and interface bodies.
    if kind != "object_type" && kind != "interface_body" {
        return;
    }

    let mut cursor = node.walk();
    let named_children: Vec<_> = node.named_children(&mut cursor).collect();

    // Must have exactly one member.
    if named_children.len() != 1 {
        return;
    }

    let member = named_children[0];
    if member.kind() != "index_signature" {
        return;
    }

    // Extract the key type and value type for the message.
    let key_type = extract_index_key_type(&member, source).unwrap_or("string");
    let value_type = extract_index_value_type(&member, source).unwrap_or("unknown");

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "ts-consistent-indexed-object-style".into(),
        message: format!("A `Record<{key_type}, {value_type}>` is preferred over an index signature."),
        severity: Severity::Warning,
        span: None,
    });
}

fn extract_index_key_type<'a>(node: &tree_sitter::Node<'a>, source: &'a [u8]) -> Option<&'a str> {
    // index_signature contains a parameter with a type annotation.
    // Pattern: [key: string]: value
    // The parameter is the first child, its type annotation has the key type.
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        // Look for the formal parameter: `key: string`
        if (child.kind() == "identifier" || child.kind() == "required_parameter")
            && let Some(type_ann) = child.child_by_field_name("type") {
                let text = std::str::from_utf8(&source[type_ann.byte_range()]).ok()?;
                return Some(text.trim().trim_start_matches(':').trim());
            }
    }
    None
}

fn extract_index_value_type<'a>(
    node: &tree_sitter::Node<'a>,
    source: &'a [u8],
) -> Option<&'a str> {
    // The index_signature has a type_annotation for the value type.
    let type_ann = node.child_by_field_name("type")?;
    let text = std::str::from_utf8(&source[type_ann.byte_range()]).ok()?;
    Some(text.trim().trim_start_matches(':').trim())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_index_signature_in_type_literal() {
        let diags = run_on("type Foo = { [key: string]: number };");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("Record"));
    }

    #[test]
    fn flags_index_signature_in_interface() {
        let diags = run_on(
            r#"
interface Foo {
    [key: string]: number;
}
"#,
        );
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn allows_multiple_members() {
        let diags = run_on(
            r#"
interface Foo {
    [key: string]: number;
    name: string;
}
"#,
        );
        assert!(diags.is_empty());
    }
}
