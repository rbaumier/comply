//! rust-pub-enum-without-non-exhaustive backend.
//!
//! Walks `enum_item` nodes with `pub` visibility and scans the
//! preceding `attribute_item` siblings for `#[non_exhaustive]`. If
//! absent, flag.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};

const KINDS: &[&str] = &["enum_item"];

#[derive(Debug)]
pub struct Check;

impl AstCheck for Check {
    fn interested_kinds(&self) -> Option<&'static [&'static str]> {
        Some(KINDS)
    }

    fn visit_node(
        &self,
        node: tree_sitter::Node,
        ctx: &CheckCtx,
        _state: Option<&mut dyn std::any::Any>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let source_bytes = ctx.source.as_bytes();
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
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "rust-pub-enum-without-non-exhaustive".into(),
            message: format!(
                "`pub enum {name}` lacks `#[non_exhaustive]` — adding \
                 a new variant later becomes a SemVer-breaking change. \
                 Add the attribute to keep the API future-proof."
            ),
            severity: Severity::Warning,
            span: None,
        });
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
    // Walk every preceding sibling; keep going through attribute_item
    // and interleaved comment nodes (tree-sitter-rust inserts
    // `line_comment`/`block_comment` siblings for trailing `//` notes).
    // Without this, a comment between `#[non_exhaustive]` and the enum
    // silently defeats detection.
    let mut sibling = item.prev_named_sibling();
    while let Some(s) = sibling {
        match s.kind() {
            "attribute_item" => {
                if let Ok(text) = s.utf8_text(source)
                    && text.contains("non_exhaustive")
                {
                    return true;
                }
            }
            "line_comment" | "block_comment" => {
                // Interleaved comment — keep walking.
            }
            _ => break,
        }
        sibling = s.prev_named_sibling();
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rust(source, &Check)
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
