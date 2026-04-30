//! no-redundant-optional backend — `?: T | undefined` repeats `| undefined`.
//!
//! Walks `property_signature`, `optional_parameter`, and `public_field_definition`
//! nodes. When the marker `?` is present and the type annotation is a `union_type`
//! containing `undefined`, the union member is redundant — `?` already implies
//! `| undefined`.

use crate::diagnostic::{Diagnostic, Severity};
use tree_sitter::Node;

/// True if `node` is a (possibly nested) `union_type` whose leaves include
/// the literal type `undefined`.
fn union_has_undefined(node: Node, source: &[u8]) -> bool {
    if node.kind() != "union_type" {
        return false;
    }
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        match child.kind() {
            "union_type" => {
                if union_has_undefined(child, source) {
                    return true;
                }
            }
            "literal_type" | "predefined_type" => {
                if std::str::from_utf8(&source[child.byte_range()]).unwrap_or("") == "undefined" {
                    return true;
                }
            }
            _ => {}
        }
    }
    false
}

/// True if `node`'s direct children include a `?` token.
fn has_question_mark(node: Node) -> bool {
    let mut cursor = node.walk();
    node.children(&mut cursor).any(|c| c.kind() == "?")
}

crate::ast_check! { on ["property_signature", "public_field_definition", "optional_parameter"] => |node, source, ctx, diagnostics|
    let kind = node.kind();
    let is_optional_holder = match kind {
        "property_signature" | "public_field_definition" => has_question_mark(node),
        "optional_parameter" => true, // optional_parameter always has `?`
        _ => return,
    };
    if !is_optional_holder {
        return;
    }

    let Some(type_ann) = ({
        let mut c = node.walk();
        node.children(&mut c).find(|n| n.kind() == "type_annotation")
    }) else {
        return;
    };

    let Some(inner) = ({
        let mut c = type_ann.walk();
        type_ann.named_children(&mut c).next()
    }) else {
        return;
    };

    if !union_has_undefined(inner, source) {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "no-redundant-optional".into(),
        message: "`?:` already implies `| undefined` — remove the redundant union member.".into(),
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
    fn flags_optional_with_undefined() {
        assert_eq!(
            run_on("interface I { name?: string | undefined; }").len(),
            1
        );
    }

    #[test]
    fn flags_optional_with_undefined_complex() {
        assert_eq!(
            run_on("interface I { value?: number | null | undefined; }").len(),
            1
        );
    }

    #[test]
    fn allows_optional_without_undefined() {
        assert!(run_on("interface I { name?: string; }").is_empty());
    }

    #[test]
    fn allows_required_with_undefined() {
        assert!(run_on("interface I { name: string | undefined; }").is_empty());
    }
}
