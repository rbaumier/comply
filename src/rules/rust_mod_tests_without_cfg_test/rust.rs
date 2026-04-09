//! rust-mod-tests-without-cfg-test backend.
//!
//! Walks `mod_item` nodes whose name is `tests` (or `test`) and
//! checks the preceding `attribute_item` siblings for a
//! `#[cfg(test)]` attribute. Flag if absent.

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
            if node.kind() != "mod_item" {
                return;
            }
            let Some(name_node) = node.child_by_field_name("name") else {
                return;
            };
            let Ok(name) = name_node.utf8_text(source_bytes) else {
                return;
            };
            if name != "tests" && name != "test" {
                return;
            }
            if has_cfg_test_attribute(node, source_bytes) {
                return;
            }
            let pos = node.start_position();
            diagnostics.push(Diagnostic {
                path: ctx.path.to_path_buf(),
                line: pos.row + 1,
                column: pos.column + 1,
                rule_id: "rust-mod-tests-without-cfg-test".into(),
                message: format!(
                    "`mod {name}` is not gated by `#[cfg(test)]` — every \
                     test function will ship in the release binary. Add \
                     `#[cfg(test)]` immediately above the module declaration."
                ),
                severity: Severity::Error,
            });
        });
        diagnostics
    }
}

fn has_cfg_test_attribute(item: tree_sitter::Node, source: &[u8]) -> bool {
    let mut sibling = item.prev_named_sibling();
    while let Some(s) = sibling {
        if s.kind() != "attribute_item" {
            break;
        }
        if let Ok(text) = s.utf8_text(source)
            && (text.contains("cfg(test)") || text.contains("cfg_attr(test"))
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
    fn flags_mod_tests_without_cfg() {
        assert_eq!(run_on("mod tests { #[test] fn t() {} }").len(), 1);
    }

    #[test]
    fn allows_mod_tests_with_cfg_test() {
        let source = "#[cfg(test)]\nmod tests { #[test] fn t() {} }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn does_not_flag_other_module() {
        assert!(run_on("mod helpers { fn h() {} }").is_empty());
    }
}
