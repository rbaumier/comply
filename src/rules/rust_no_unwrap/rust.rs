//! rust-no-unwrap backend.
//!
//! Flags `.unwrap()` and `.expect(...)` method calls in non-test code.
//! These turn runtime conditions (None / Err) into panics, which is the
//! opposite of what production code should do. Prefer `?` + proper error
//! types, or `unwrap_or_else` with a meaningful fallback.
//!
//! Tests are exempted — `.unwrap()` in a unit test is idiomatic because
//! a panic cleanly fails the test. We skip any call whose enclosing
//! function has `#[test]` or whose enclosing module has `#[cfg(test)]`.
//!
//! This rule is equivalent to `clippy::unwrap_used` + `clippy::expect_used`
//! (both restriction-group lints, off by default in clippy). Running it
//! via comply means you get the check without having to enable the lints
//! in every consuming crate.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};
use crate::rules::walker::walk_tree;

pub struct Check;

impl AstCheck for Check {
    fn check(&self, ctx: &CheckCtx, tree: &tree_sitter::Tree) -> Vec<Diagnostic> {
        let source_bytes = ctx.source.as_bytes();
        let mut diagnostics = Vec::new();
        walk_tree(tree, |node| {
            // Looking for `receiver.unwrap()` / `receiver.expect("…")`.
            if node.kind() != "call_expression" {
                return;
            }
            let Some(function) = node.child_by_field_name("function") else {
                return;
            };
            if function.kind() != "field_expression" {
                return;
            }
            let Some(field) = function.child_by_field_name("field") else {
                return;
            };
            let Ok(field_text) = field.utf8_text(source_bytes) else {
                return;
            };
            if field_text != "unwrap" && field_text != "expect" {
                return;
            }
            // Skip test code — `.unwrap()` is fine there.
            if is_in_test_context(node, source_bytes) {
                return;
            }
            let pos = node.start_position();
            diagnostics.push(Diagnostic {
                path: ctx.path.to_path_buf(),
                line: pos.row + 1,
                column: pos.column + 1,
                rule_id: "rust-no-unwrap".into(),
                message: format!(
                    "`.{field_text}()` turns a runtime condition into a panic. \
                     Use `?` with a proper error type, or `unwrap_or_else` with \
                     a meaningful fallback. Tests are exempted."
                ),
                severity: Severity::Error,
            });
        });
        diagnostics
    }
}

/// True if `node` is inside a `#[test]` function or a `#[cfg(test)]` module.
/// Walks up the parent chain looking for function/module items with those
/// attributes on an `attribute_item` sibling directly above the declaration.
fn is_in_test_context(node: tree_sitter::Node, source: &[u8]) -> bool {
    let mut cur = node;
    while let Some(parent) = cur.parent() {
        if (parent.kind() == "function_item" || parent.kind() == "mod_item")
            && has_test_attribute(parent, source)
        {
            return true;
        }
        cur = parent;
    }
    false
}

/// True if the item has `#[test]` or `#[cfg(test)]` as a preceding
/// `attribute_item` sibling. In tree-sitter-rust, attributes on an item
/// appear as `attribute_item` nodes immediately before the item.
fn has_test_attribute(item: tree_sitter::Node, source: &[u8]) -> bool {
    let mut sibling = item.prev_named_sibling();
    while let Some(s) = sibling {
        if s.kind() != "attribute_item" {
            break;
        }
        if let Ok(text) = s.utf8_text(source)
            && (text.contains("#[test]")
                || text.contains("cfg(test)")
                || text.contains("cfg_attr(test"))
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
    fn flags_unwrap_in_production_fn() {
        assert_eq!(run_on("fn f() { let x = y.unwrap(); }").len(), 1);
    }

    #[test]
    fn flags_expect_in_production_fn() {
        assert_eq!(run_on(r#"fn f() { let x = y.expect("msg"); }"#).len(), 1);
    }

    #[test]
    fn allows_unwrap_in_test_function() {
        let source = "#[test]\nfn it_works() { let x = y.unwrap(); }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_unwrap_inside_cfg_test_module() {
        let source = "#[cfg(test)]\nmod tests { fn f() { let x = y.unwrap(); } }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn does_not_flag_question_mark() {
        assert!(run_on("fn f() -> Result<(), ()> { let x = y?; Ok(()) }").is_empty());
    }
}
