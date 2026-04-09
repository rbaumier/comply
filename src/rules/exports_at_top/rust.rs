//! exports-at-top backend for Rust.
//!
//! Rust semantics: `pub` items should appear before private items at
//! module scope. A reader opening the file should see the public API
//! at the top. No clippy equivalent.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};

/// Item kinds at module scope that have a visibility (pub / private).
const ITEM_KINDS: &[&str] = &[
    "function_item",
    "struct_item",
    "enum_item",
    "trait_item",
    "impl_item",
    "const_item",
    "static_item",
    "type_item",
    "mod_item",
    "use_declaration",
];

pub struct Check;

impl AstCheck for Check {
    fn check(&self, ctx: &CheckCtx, tree: &tree_sitter::Tree) -> Vec<Diagnostic> {
        let source_bytes = ctx.source.as_bytes();
        let root = tree.root_node();
        let mut seen_private = false;
        let mut diagnostics = Vec::new();

        let mut cursor = root.walk();
        for child in root.children(&mut cursor) {
            if !ITEM_KINDS.contains(&child.kind()) {
                continue;
            }
            let is_public = has_pub_visibility(child, source_bytes);
            if is_public && seen_private {
                let pos = child.start_position();
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: pos.row + 1,
                    column: pos.column + 1,
                    rule_id: "exports-at-top".into(),
                    message: "Public item declared after a private item — \
                              move all `pub` items above the private \
                              helpers so the module's API is visible at a glance."
                        .into(),
                    severity: Severity::Warning,
                });
            }
            if !is_public {
                seen_private = true;
            }
        }
        diagnostics
    }
}

fn has_pub_visibility(node: tree_sitter::Node, source: &[u8]) -> bool {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "visibility_modifier"
            && child.utf8_text(source).is_ok_and(|t| t.starts_with("pub"))
        {
            return true;
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
            &CheckCtx {
                path: Path::new("t.rs"),
                source,
            },
            &tree,
        )
    }

    #[test]
    fn allows_public_then_private() {
        let source = "pub fn a() {}\npub fn b() {}\nfn helper() {}\n";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn flags_public_after_private() {
        let source = "fn helper() {}\npub fn exposed() {}\n";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn allows_all_public() {
        assert!(run_on("pub fn a() {}\npub fn b() {}\n").is_empty());
    }

    #[test]
    fn allows_all_private() {
        assert!(run_on("fn a() {}\nfn b() {}\n").is_empty());
    }
}
