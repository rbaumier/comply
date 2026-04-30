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

#[derive(Debug)]
pub struct Check;

impl AstCheck for Check {
    fn interested_kinds(&self) -> Option<&'static [&'static str]> {
        Some(&["struct_item", "enum_item"])
    }

    fn visit_node(
        &self,
        node: tree_sitter::Node,
        ctx: &CheckCtx,
        _state: Option<&mut dyn std::any::Any>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let source_bytes = ctx.source.as_bytes();
        let source_str = ctx.source;
        let kind = node.kind();
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
        let kind_label = if kind == "struct_item" {
            "struct"
        } else {
            "enum"
        };
        diagnostics.push(Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
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

fn has_debug_derive(item: tree_sitter::Node, source: &[u8]) -> bool {
    // Walk every preceding sibling; keep going through attribute_item
    // and comment nodes (both `line_comment` and `block_comment`, which
    // tree-sitter-rust inserts between attributes when a trailing `//`
    // or block comment sits beside an attribute like
    // `#[allow(...)] // trailing note`). Stop at the first sibling that
    // isn't an attribute or a comment — that's where our declaration's
    // attribute block actually ends.
    let mut sibling = item.prev_named_sibling();
    while let Some(s) = sibling {
        match s.kind() {
            "attribute_item" => {
                if let Ok(text) = s.utf8_text(source)
                    && text.contains("derive(")
                    && text.contains("Debug")
                {
                    return true;
                }
            }
            "line_comment" | "block_comment" => {
                // Comments interleaved with attributes don't end the
                // attribute block. Keep walking.
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
            run_on("#[derive(Clone, Debug, Default)]\npub struct User { name: String }").is_empty()
        );
    }

    #[test]
    fn allows_pub_struct_with_manual_debug_impl() {
        let source = "pub struct Closure { f: Box<dyn Fn()> }\nimpl Debug for Closure { fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result { Ok(()) } }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn does_not_flag_private_struct() {
        assert!(run_on("struct User { name: String }").is_empty());
    }

    #[test]
    fn allows_doc_comment_above_multi_attribute_block() {
        // Reproduces the RuleMeta false positive: a doc comment, then
        // `#[derive(Debug, ...)]`, then another `#[allow(...)]`, then
        // the struct. The walker must traverse both attribute items
        // without being stopped by the preceding doc comment.
        let source = "/// Doc line 1.\n\
                      /// Doc line 2.\n\
                      #[derive(Debug, Clone, Copy)]\n\
                      #[allow(dead_code)]\n\
                      pub struct RuleMeta { pub id: &'static str }";
        assert!(
            run_on(source).is_empty(),
            "false positive: multi-attribute block with Debug derive should not fire"
        );
    }

    #[test]
    fn allows_trailing_comment_after_inner_attribute() {
        // Reproduces the exact RuleMeta shape in meta.rs: a trailing
        // `// comment` after `#[allow(dead_code)]` between the derive
        // and the struct. tree-sitter-rust may split this differently.
        let source = "/// Doc.\n\
                      #[derive(Debug, Clone, Copy)]\n\
                      #[allow(dead_code)] // Fields read by JSON output / explain / remap (coming soon).\n\
                      pub struct RuleMeta { pub id: &'static str }";
        assert!(
            run_on(source).is_empty(),
            "false positive: trailing line comment after attribute should not defeat Debug detection"
        );
    }
}
