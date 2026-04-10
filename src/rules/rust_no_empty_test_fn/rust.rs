//! rust-no-empty-test-fn backend.
//!
//! Walks `function_item` nodes whose preceding `attribute_item`
//! sibling carries `#[test]` and whose body block has zero named
//! children (no statements, no expressions). The body still has
//! the `{` and `}` punctuation tokens, but those are anonymous
//! children — `named_child_count() == 0` is the right check.

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
            if !has_test_attribute(node, source_bytes) {
                return;
            }
            let Some(body) = node.child_by_field_name("body") else {
                return;
            };
            if body.named_child_count() != 0 {
                return;
            }
            let name = node
                .child_by_field_name("name")
                .and_then(|n| n.utf8_text(source_bytes).ok())
                .unwrap_or("test");
            let pos = node.start_position();
            diagnostics.push(Diagnostic {
                path: ctx.path.to_path_buf(),
                line: pos.row + 1,
                column: pos.column + 1,
                rule_id: "rust-no-empty-test-fn".into(),
                message: format!(
                    "`#[test] fn {name}` has an empty body — it always \
                     passes without exercising any code. Fill it in or \
                     delete it."
                ),
                severity: Severity::Error,
            });
        });
        diagnostics
    }
}

fn has_test_attribute(item: tree_sitter::Node, source: &[u8]) -> bool {
    let mut sibling = item.prev_named_sibling();
    while let Some(s) = sibling {
        if s.kind() != "attribute_item" {
            break;
        }
        if let Ok(text) = s.utf8_text(source)
            && text.contains("#[test]")
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
    

    fn run_on(source: &str) -> Vec<Diagnostic> {


        crate::rules::test_helpers::run_rust(source, &Check)


    }

    #[test]
    fn flags_empty_test_fn() {
        let source = "#[test]\nfn it_works() {}";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn allows_test_with_assertion() {
        let source = "#[test]\nfn it_works() { assert_eq!(1 + 1, 2); }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn does_not_flag_empty_non_test_fn() {
        // Empty non-test fns are someone else's problem (no_empty_function rule).
        assert!(run_on("fn placeholder() {}").is_empty());
    }
}
