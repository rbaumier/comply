//! rust-impl-debug-on-public-types backend.
//!
//! For every `struct_item` and `enum_item` with a `pub` visibility
//! modifier, scan the preceding `attribute_item` siblings looking
//! for either `#[derive(...Debug...)]` or a manual `impl Debug for
//! ...` somewhere in the file. Flag if neither is present.
//!
//! We accept manual impls because libraries with closure or PhantomData
//! fields legitimately can't derive — they hand-roll the impl. The
//! file-wide check is a heuristic but matches real codebases.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};
use crate::rules::walker::walk_tree;

#[derive(Debug)]
pub struct Check;

impl AstCheck for Check {
    fn check(&self, ctx: &CheckCtx, tree: &tree_sitter::Tree) -> Vec<Diagnostic> {
        let source_bytes = ctx.source.as_bytes();
        let source_str = ctx.source;
        let mut diagnostics = Vec::new();
        walk_tree(tree, |node| {
            let kind = node.kind();
            if kind != "struct_item" && kind != "enum_item" {
                return;
            }
            if !is_pub(node, source_bytes) {
                return;
            }
            let Some(name_node) = node.child_by_field_name("name") else {
                return;
            };
            let Ok(name) = name_node.utf8_text(source_bytes) else {
                return;
            };
            if has_debug_derive(node, source_bytes) {
                return;
            }
            // Manual `impl Debug for Name` anywhere in the file.
            if source_str.contains(&format!("impl Debug for {name}"))
                || source_str.contains(&format!("impl std::fmt::Debug for {name}"))
                || source_str.contains(&format!("impl fmt::Debug for {name}"))
            {
                return;
            }
            let pos = node.start_position();
            let kind_label = if kind == "struct_item" { "struct" } else { "enum" };
            diagnostics.push(Diagnostic {
                path: ctx.path.to_path_buf(),
                line: pos.row + 1,
                column: pos.column + 1,
                rule_id: "rust-impl-debug-on-public-types".into(),
                message: format!(
                    "`pub {kind_label} {name}` has no `Debug` impl — \
                     consumers can't log it, can't use it in assert \
                     failure messages, can't see it in `{{:?}}` output. \
                     Add `#[derive(Debug)]` or implement `Debug` by hand."
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

fn has_debug_derive(item: tree_sitter::Node, source: &[u8]) -> bool {
    let mut sibling = item.prev_named_sibling();
    while let Some(s) = sibling {
        if s.kind() != "attribute_item" {
            break;
        }
        if let Ok(text) = s.utf8_text(source)
            && text.contains("derive(")
            && text.contains("Debug")
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
    fn flags_pub_struct_without_debug() {
        assert_eq!(run_on("pub struct User { name: String }").len(), 1);
    }

    #[test]
    fn flags_pub_enum_without_debug() {
        assert_eq!(run_on("pub enum State { Idle, Busy }").len(), 1);
    }

    #[test]
    fn allows_pub_struct_with_debug_derive() {
        assert!(run_on("#[derive(Debug)]\npub struct User { name: String }").is_empty());
    }

    #[test]
    fn allows_pub_struct_with_mixed_derive() {
        assert!(
            run_on("#[derive(Clone, Debug, Default)]\npub struct User { name: String }")
                .is_empty()
        );
    }

    #[test]
    fn allows_pub_struct_with_manual_debug_impl() {
        let source =
            "pub struct Closure { f: Box<dyn Fn()> }\nimpl Debug for Closure { fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result { Ok(()) } }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn does_not_flag_private_struct() {
        assert!(run_on("struct User { name: String }").is_empty());
    }
}
