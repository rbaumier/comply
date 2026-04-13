//! ts-no-unused-private-class-members backend — detect private class
//! members (ES `#private` and TS `private` keyword) that are declared
//! but never referenced within the class body.
//!
//! Simplified approach: for each class, collect private member names
//! from declarations, then scan the class body for references.

use std::collections::HashMap;
use crate::diagnostic::{Diagnostic, Severity};

/// Collect all text content of a node's subtree for reference scanning.
fn collect_text_references(node: tree_sitter::Node, source: &[u8], refs: &mut Vec<String>) {
    match node.kind() {
        "property_identifier" | "private_property_identifier" | "identifier" => {
            if let Ok(text) = node.utf8_text(source) {
                refs.push(text.to_string());
            }
        }
        _ => {}
    }
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        collect_text_references(child, source, refs);
    }
}

/// Check if a class member has a `private` accessibility modifier.
fn has_private_modifier(member: tree_sitter::Node, source: &[u8]) -> bool {
    let mut cursor = member.walk();
    for child in member.children(&mut cursor) {
        if child.kind() == "accessibility_modifier"
            && let Ok(text) = child.utf8_text(source)
                && text == "private" {
                    return true;
                }
    }
    false
}

/// Check if the field/method name is an ES private identifier (#foo).
fn is_es_private_name(member: tree_sitter::Node) -> bool {
    if let Some(name_node) = member.child_by_field_name("name") {
        return name_node.kind() == "private_property_identifier";
    }
    // Also check "property" field name (some grammar versions)
    if let Some(prop) = member.child_by_field_name("property") {
        return prop.kind() == "private_property_identifier";
    }
    false
}

/// Get the name of a class member (from "name" or "property" field).
fn member_name<'a>(member: tree_sitter::Node<'a>, source: &'a [u8]) -> Option<(&'a str, tree_sitter::Point)> {
    for field in &["name", "property"] {
        if let Some(name_node) = member.child_by_field_name(field)
            && let Ok(name) = name_node.utf8_text(source) {
                return Some((name, name_node.start_position()));
            }
    }
    None
}

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "class_declaration" && node.kind() != "class" {
        return;
    }

    let Some(body) = node.child_by_field_name("body") else {
        return;
    };

    // Phase 1: collect private member declarations
    let mut private_members: HashMap<String, tree_sitter::Point> = HashMap::new();
    // Phase 2: collect all references in method bodies and property initializers
    let mut all_references: Vec<String> = Vec::new();

    let mut cursor = body.walk();
    for member in body.named_children(&mut cursor) {
        match member.kind() {
            "public_field_definition" | "field_definition" | "property_definition" => {
                let is_private = has_private_modifier(member, source) || is_es_private_name(member);

                if is_private
                    && let Some((name, pos)) = member_name(member, source)
                        && name != "constructor" {
                            private_members.entry(name.to_string()).or_insert(pos);
                        }
                // Collect references from the initializer value
                if let Some(value) = member.child_by_field_name("value") {
                    collect_text_references(value, source, &mut all_references);
                }
            }
            "method_definition" => {
                let is_private = has_private_modifier(member, source) || is_es_private_name(member);

                if is_private
                    && let Some((name, pos)) = member_name(member, source)
                        && name != "constructor" {
                            private_members.entry(name.to_string()).or_insert(pos);
                        }

                // Collect references from method body
                if let Some(body_node) = member.child_by_field_name("body") {
                    collect_text_references(body_node, source, &mut all_references);
                }
            }
            _ => {
                collect_text_references(member, source, &mut all_references);
            }
        }
    }

    // Phase 3: flag private members with no references
    for (name, pos) in &private_members {
        let ref_count = all_references.iter().filter(|r| r.as_str() == name.as_str()).count();
        if ref_count == 0 {
            diagnostics.push(Diagnostic {
                path: ctx.path.to_path_buf(),
                line: pos.row + 1,
                column: pos.column + 1,
                rule_id: "ts-no-unused-private-class-members".into(),
                message: format!("Private member `{name}` is declared but never used."),
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
    fn flags_unused_es_private_field() {
        let d = run_on("class A { #unused = 42; }");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("#unused"));
    }

    #[test]
    fn allows_used_es_private_field() {
        let d = run_on("class A { #foo = 42; method() { return this.#foo; } }");
        assert!(d.is_empty());
    }

    #[test]
    fn flags_unused_private_method() {
        let d = run_on("class A { #unused() {} doStuff() { return 1; } }");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("#unused"));
    }
}
