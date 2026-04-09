//! rust-serde-deny-unknown-fields backend.
//!
//! For every `struct_item` with a `#[derive(..., Deserialize, ...)]`
//! attribute, scan the preceding attribute siblings for
//! `#[serde(deny_unknown_fields)]`. If absent, flag the struct.
//!
//! **Exception:** a struct with any `#[serde(flatten)]` field is
//! deliberately NOT flagged. `deny_unknown_fields` and `flatten` are
//! mutually exclusive in serde — the flatten's target HashMap/struct
//! is exactly the mechanism for accepting unknown keys, so rejecting
//! them before the flatten can catch them defeats the field's purpose.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};
use crate::rules::walker::walk_tree;

#[derive(Debug)]
pub struct Check;

impl AstCheck for Check {
    fn check(&self, ctx: &CheckCtx, tree: &tree_sitter::Tree) -> Vec<Diagnostic> {
        let source_bytes = ctx.source.as_bytes();
        let mut diagnostics = Vec::new();
        walk_tree(tree, |node| {
            if node.kind() != "struct_item" {
                return;
            }
            let attrs = collect_preceding_attrs(node, source_bytes);
            if !attrs.iter().any(|a| derives_deserialize(a)) {
                return;
            }
            if attrs.iter().any(|a| has_deny_unknown_fields(a)) {
                return;
            }
            // Structs with a `#[serde(flatten)]` field cannot have
            // `deny_unknown_fields` — the two are mutually exclusive.
            if has_flatten_field(node, source_bytes) {
                return;
            }
            let name = node
                .child_by_field_name("name")
                .and_then(|n| n.utf8_text(source_bytes).ok())
                .unwrap_or("Struct");
            let pos = node.start_position();
            diagnostics.push(Diagnostic {
                path: ctx.path.to_path_buf(),
                line: pos.row + 1,
                column: pos.column + 1,
                rule_id: "rust-serde-deny-unknown-fields".into(),
                message: format!(
                    "`{name}` derives `Deserialize` but is missing \
                     `#[serde(deny_unknown_fields)]` — typos in input \
                     fields will be silently dropped. Add the attribute \
                     to catch unknown keys at parse time."
                ),
                severity: Severity::Warning,
            });
        });
        diagnostics
    }
}

fn collect_preceding_attrs(item: tree_sitter::Node, source: &[u8]) -> Vec<String> {
    // Walk every preceding sibling; keep going through attribute_item
    // and interleaved comment nodes. tree-sitter-rust inserts a
    // `line_comment`/`block_comment` sibling whenever an attribute has
    // a trailing `//` note (e.g. `#[allow(dead_code)] // explanation`),
    // so stopping at the first non-attribute would prematurely end the
    // block and miss derives sitting above it.
    let mut out = Vec::new();
    let mut sibling = item.prev_named_sibling();
    while let Some(s) = sibling {
        match s.kind() {
            "attribute_item" => {
                if let Ok(text) = s.utf8_text(source) {
                    out.push(text.to_string());
                }
            }
            "line_comment" | "block_comment" => {
                // Interleaved comment — keep walking.
            }
            _ => break,
        }
        sibling = s.prev_named_sibling();
    }
    out
}

fn derives_deserialize(attr_text: &str) -> bool {
    // Match `#[derive(..., Deserialize, ...)]` — we don't enforce
    // word boundaries because `MyDeserialize` would be a very strange
    // name to invent.
    attr_text.contains("derive(") && attr_text.contains("Deserialize")
}

fn has_deny_unknown_fields(attr_text: &str) -> bool {
    attr_text.contains("deny_unknown_fields")
}

/// True if any field inside the struct body carries a
/// `#[serde(flatten)]` attribute. We walk the `field_declaration_list`
/// child and, for each `field_declaration`, look for preceding
/// `attribute_item` siblings containing `flatten`.
fn has_flatten_field(struct_node: tree_sitter::Node, source: &[u8]) -> bool {
    let Some(body) = struct_node.child_by_field_name("body") else {
        return false;
    };
    if body.kind() != "field_declaration_list" {
        return false;
    }
    let mut cursor = body.walk();
    for field in body.children(&mut cursor) {
        if field.kind() != "field_declaration" {
            continue;
        }
        let mut sibling = field.prev_named_sibling();
        while let Some(s) = sibling {
            match s.kind() {
                "attribute_item" => {
                    if let Ok(text) = s.utf8_text(source)
                        && text.contains("flatten")
                    {
                        return true;
                    }
                }
                "line_comment" | "block_comment" => {}
                _ => break,
            }
            sibling = s.prev_named_sibling();
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        let mut parser = tree_sitter::Parser::new();
        parser.set_language(&tree_sitter_rust::LANGUAGE.into()).unwrap();
        let tree = parser.parse(source, None).unwrap();
        Check.check(
            &CheckCtx::for_test(Path::new("t.rs"), source),
            &tree,
        )
    }

    #[test]
    fn flags_deserialize_without_deny_unknown_fields() {
        let source = "#[derive(Deserialize)]\nstruct Config { rate: u32 }";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn allows_deserialize_with_deny_unknown_fields() {
        let source =
            "#[derive(Deserialize)]\n#[serde(deny_unknown_fields)]\nstruct Config { rate: u32 }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_struct_without_deserialize() {
        let source = "#[derive(Debug)]\nstruct Config { rate: u32 }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn flags_mixed_derive_with_deserialize() {
        let source = "#[derive(Debug, Clone, Deserialize, Serialize)]\nstruct Config { rate: u32 }";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn allows_struct_with_flatten_field() {
        // `deny_unknown_fields` and `#[serde(flatten)]` are mutually
        // exclusive — the flatten is how you accept unknown keys.
        let source = "#[derive(Deserialize)]\n\
                      struct Config {\n\
                          name: String,\n\
                          #[serde(flatten)]\n\
                          extra: std::collections::HashMap<String, toml::Value>,\n\
                      }";
        assert!(
            run_on(source).is_empty(),
            "false positive: struct with flatten field can't have deny_unknown_fields"
        );
    }
}
