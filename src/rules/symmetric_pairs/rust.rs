//! symmetric-pairs Rust backend.
//!
//! Check `pub fn` items for missing symmetric counterparts.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};
use crate::rules::walker::walk_tree;

const PAIRS: &[(&str, &str)] = &[
    ("get_", "set_"),
    ("set_", "get_"),
    ("add_", "remove_"),
    ("remove_", "add_"),
    ("open_", "close_"),
    ("close_", "open_"),
    ("start_", "stop_"),
    ("stop_", "start_"),
    ("create_", "delete_"),
    ("delete_", "create_"),
    ("create_", "destroy_"),
];

const PREFIXES: &[&str] = &[
    "get_", "set_", "add_", "remove_", "open_", "close_", "start_", "stop_", "create_",
    "delete_", "destroy_",
];

fn split_prefix(name: &str) -> Option<(&str, &str)> {
    for &pfx in PREFIXES {
        if name.len() > pfx.len() && name.starts_with(pfx) {
            return Some((pfx, &name[pfx.len()..]));
        }
    }
    None
}

#[derive(Debug)]
pub struct Check;

impl AstCheck for Check {
    fn check(&self, ctx: &CheckCtx, tree: &tree_sitter::Tree) -> Vec<Diagnostic> {
        let source = ctx.source.as_bytes();
        let mut pub_fns: Vec<String> = Vec::new();

        // Collect all `pub fn` names.
        walk_tree(tree, |node| {
            if node.kind() != "function_item" {
                return;
            }
            let Ok(text) = node.utf8_text(source) else { return };
            if !text.starts_with("pub ") {
                return;
            }
            if let Some(name_node) = node.child_by_field_name("name")
                && let Ok(name) = name_node.utf8_text(source) {
                    pub_fns.push(name.to_string());
                }
        });

        let mut diagnostics = Vec::new();
        walk_tree(tree, |node| {
            if node.kind() != "function_item" {
                return;
            }
            let Ok(text) = node.utf8_text(source) else { return };
            if !text.starts_with("pub ") {
                return;
            }
            let Some(name_node) = node.child_by_field_name("name") else { return };
            let Ok(name) = name_node.utf8_text(source) else { return };
            let Some((prefix, suffix)) = split_prefix(name) else { return };

            for &(pfx, counterpart_pfx) in PAIRS {
                if pfx == prefix {
                    let expected = format!("{counterpart_pfx}{suffix}");
                    if !pub_fns.contains(&expected) {
                        let pos = name_node.start_position();
                        diagnostics.push(Diagnostic {
                            path: ctx.path.to_path_buf(),
                            line: pos.row + 1,
                            column: pos.column + 1,
                            rule_id: "symmetric-pairs".into(),
                            message: format!(
                                "`pub fn {name}` has no `{expected}` counterpart."
                            ),
                            severity: Severity::Warning,
                        });
                        break;
                    }
                }
            }
        });
        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rust(source, &Check)
    }

    #[test]
    fn flags_missing_counterpart() {
        let src = "pub fn open_connection() {}\n";
        let d = run_on(src);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("close_connection"));
    }

    #[test]
    fn allows_complete_pair() {
        let src = "pub fn open_connection() {}\npub fn close_connection() {}\n";
        assert!(run_on(src).is_empty());
    }
}
