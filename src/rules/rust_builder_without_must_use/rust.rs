//! rust-builder-without-must-use backend.
//!
//! Heuristic: any `struct_item` whose name ends in `Builder` (the
//! near-universal Rust convention for the builder pattern) must
//! carry a `#[must_use]` attribute. The check is name-based on
//! purpose — detecting builder shape from a single struct definition
//! is unreliable, but the naming convention is strong enough to
//! catch the real cases without false positives.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};
use crate::rules::walker::walk_tree;

pub struct Check;

impl AstCheck for Check {
    fn check(&self, ctx: &CheckCtx, tree: &tree_sitter::Tree) -> Vec<Diagnostic> {
        let source_bytes = ctx.source.as_bytes();
        let mut diagnostics = Vec::new();
        walk_tree(tree, |node| {
            if node.kind() != "struct_item" {
                return;
            }
            let Some(name_node) = node.child_by_field_name("name") else {
                return;
            };
            let Ok(name) = name_node.utf8_text(source_bytes) else {
                return;
            };
            if !name.ends_with("Builder") {
                return;
            }
            if has_must_use_attribute(node, source_bytes) {
                return;
            }
            let pos = node.start_position();
            diagnostics.push(Diagnostic {
                path: ctx.path.to_path_buf(),
                line: pos.row + 1,
                column: pos.column + 1,
                rule_id: "rust-builder-without-must-use".into(),
                message: format!(
                    "`{name}` looks like a builder but has no `#[must_use]`. \
                     Without it, a caller who forgets `.build()` gets a \
                     silent no-op."
                ),
                severity: Severity::Warning,
            });
        });
        diagnostics
    }
}

fn has_must_use_attribute(item: tree_sitter::Node, source: &[u8]) -> bool {
    let mut sibling = item.prev_named_sibling();
    while let Some(s) = sibling {
        if s.kind() != "attribute_item" {
            break;
        }
        if let Ok(text) = s.utf8_text(source)
            && text.contains("must_use")
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
            &CheckCtx {
                path: Path::new("t.rs"),
                source,
            },
            &tree,
        )
    }

    #[test]
    fn flags_builder_without_must_use() {
        assert_eq!(run_on("struct RequestBuilder { headers: Vec<String> }").len(), 1);
    }

    #[test]
    fn allows_builder_with_must_use() {
        let source = "#[must_use]\nstruct RequestBuilder { headers: Vec<String> }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_non_builder_struct() {
        assert!(run_on("struct Request { url: String }").is_empty());
    }
}
