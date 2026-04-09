//! rust-must-use-on-result backend.
//!
//! Flags `pub fn ... -> Result<..>` signatures that don't have a
//! `#[must_use]` attribute. Without it, callers can silently discard
//! the Result and lose every error — the exact behavior `Result`
//! exists to prevent. The attribute makes the compiler shout when the
//! return value is dropped on the floor.
//!
//! Only applies to bare `pub fn` items — trait impl methods inherit
//! their visibility from the trait and can't carry `#[must_use]`
//! independently, so we skip them.

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
            if node.kind() != "function_item" {
                return;
            }
            if !is_pub(node, source_bytes) {
                return;
            }
            if !returns_result(node, source_bytes) {
                return;
            }
            if has_must_use_attribute(node, source_bytes) {
                return;
            }
            // Skip trait impl methods — they inherit visibility from the trait.
            if is_inside_trait_impl(node) {
                return;
            }
            let pos = node.start_position();
            diagnostics.push(Diagnostic {
                path: ctx.path.to_path_buf(),
                line: pos.row + 1,
                column: pos.column + 1,
                rule_id: "rust-must-use-on-result".into(),
                message: "`pub fn` returning `Result` without `#[must_use]` — \
                          callers can silently drop the Result and lose every \
                          error. Add `#[must_use]` above the signature."
                    .into(),
                severity: Severity::Warning,
            });
        });
        diagnostics
    }
}

/// True if the function has a `visibility_modifier` child whose text
/// starts with `pub` (covers `pub`, `pub(crate)`, `pub(super)`, etc.).
fn is_pub(func: tree_sitter::Node, source: &[u8]) -> bool {
    let mut cursor = func.walk();
    for child in func.children(&mut cursor) {
        if child.kind() == "visibility_modifier"
            && let Ok(text) = child.utf8_text(source)
            && text.starts_with("pub")
        {
            return true;
        }
    }
    false
}

/// True if the function's return type is `Result<..>`. Matches either
/// a bare `Result<...>` or a qualified path ending in `::Result<...>`.
fn returns_result(func: tree_sitter::Node, source: &[u8]) -> bool {
    let Some(ret_type) = func.child_by_field_name("return_type") else {
        return false;
    };
    type_is_result(ret_type, source)
}

fn type_is_result(node: tree_sitter::Node, source: &[u8]) -> bool {
    // Bare `Result<...>` — generic_type with type field = type_identifier "Result".
    if node.kind() == "generic_type"
        && let Some(type_node) = node.child_by_field_name("type")
    {
        let Ok(text) = type_node.utf8_text(source) else {
            return false;
        };
        // Accept `Result`, `std::result::Result`, `io::Result`, etc.
        return text == "Result" || text.ends_with("::Result");
    }
    false
}

/// True if a preceding `attribute_item` sibling contains `#[must_use]`.
fn has_must_use_attribute(func: tree_sitter::Node, source: &[u8]) -> bool {
    let mut sibling = func.prev_named_sibling();
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

/// True if the function is inside an `impl Trait for Type { ... }` block.
/// tree-sitter-rust gives impl items the `impl_item` kind; the trait name
/// lives in the `trait` field. If that field is present, it's a trait impl.
fn is_inside_trait_impl(node: tree_sitter::Node) -> bool {
    let mut cur = node;
    while let Some(parent) = cur.parent() {
        if parent.kind() == "impl_item" && parent.child_by_field_name("trait").is_some() {
            return true;
        }
        cur = parent;
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
    fn flags_pub_fn_returning_result_without_must_use() {
        let source = "pub fn f() -> Result<(), Error> { Ok(()) }";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn allows_pub_fn_with_must_use() {
        let source = "#[must_use]\npub fn f() -> Result<(), Error> { Ok(()) }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn does_not_flag_private_fn() {
        let source = "fn f() -> Result<(), Error> { Ok(()) }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn does_not_flag_non_result_return() {
        let source = "pub fn f() -> i32 { 42 }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn does_not_flag_trait_impl_method() {
        let source = "impl MyTrait for Foo {\n\
                      fn f() -> Result<(), Error> { Ok(()) }\n\
                      }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn flags_inherent_impl_method() {
        // Inherent impl (no trait) — method needs #[must_use] like a free fn.
        let source = "impl Foo {\n\
                      pub fn f() -> Result<(), Error> { Ok(()) }\n\
                      }";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn recognises_io_result() {
        let source = "pub fn f() -> io::Result<()> { Ok(()) }";
        assert_eq!(run_on(source).len(), 1);
    }
}
