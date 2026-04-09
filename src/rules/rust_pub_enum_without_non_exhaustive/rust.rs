//! rust-pub-enum-without-non-exhaustive backend.
//!
//! Walks `enum_item` nodes with `pub` visibility and scans the
//! preceding `attribute_item` siblings for `#[non_exhaustive]`. If
//! absent, flag.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};
use crate::rules::walker::walk_tree;

pub struct Check;

impl AstCheck for Check {
    fn check(&self, ctx: &CheckCtx, tree: &tree_sitter::Tree) -> Vec<Diagnostic> {
        let source_bytes = ctx.source.as_bytes();
        let mut diagnostics = Vec::new();
        walk_tree(tree, |node| {
            if node.kind() != "enum_item" {
                return;
            }
            if !is_pub(node, source_bytes) {
                return;
            }
            if has_non_exhaustive(node, source_bytes) {
                return;
            }
            let name = node
                .child_by_field_name("name")
                .and_then(|n| n.utf8_text(source_bytes).ok())
                .unwrap_or("Enum");
            let pos = node.start_position();
            diagnostics.push(Diagnostic {
                path: ctx.path.to_path_buf(),
                line: pos.row + 1,
                column: pos.column + 1,
                rule_id: "rust-pub-enum-without-non-exhaustive".into(),
                message: format!(
                    "`pub enum {name}` lacks `#[non_exhaustive]` — adding \
                     a new variant later becomes a SemVer-breaking change. \
                     Add the attribute to keep the API future-proof."
                ),
                severity: Severity::Warning,
            });
        });
        diagnostics
    }
}

fn is_pub(item: tree_sitter::Node, source: &[u8]) -> bool {
    let mut cursor = item.walk();
    for child in item.children(&mut cursor) {
        if child.kind() == "visibility_modifier"
            && let Ok(text) = child.utf8_text(source)
            && text.starts_with("pub")
        {
            return true;
        }
    }
    false
}

fn has_non_exhaustive(item: tree_sitter::Node, source: &[u8]) -> bool {
    let mut sibling = item.prev_named_sibling();
    while let Some(s) = sibling {
        if s.kind() != "attribute_item" {
            break;
        }
        if let Ok(text) = s.utf8_text(source)
            && text.contains("non_exhaustive")
        {
            return true;
        }
        sibling = s.prev_named_sibling();
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
    fn flags_pub_enum_without_non_exhaustive() {
        assert_eq!(run_on("pub enum Status { Ok, Err }").len(), 1);
    }

    #[test]
    fn allows_pub_enum_with_non_exhaustive() {
        let source = "#[non_exhaustive]\npub enum Status { Ok, Err }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn does_not_flag_private_enum() {
        assert!(run_on("enum Status { Ok, Err }").is_empty());
    }
}
