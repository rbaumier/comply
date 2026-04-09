//! rust-no-panic-macros backend.
//!
//! Flags invocations of `panic!`, `todo!`, `unimplemented!`, and
//! `unreachable!` outside of test code. These macros all abort at
//! runtime — the opposite of what a production service should do.
//!
//! - `panic!` — turn it into a typed `Result` error.
//! - `todo!` / `unimplemented!` — placeholders that must not ship.
//! - `unreachable!` — only legitimate when marking a compiler-proven
//!   impossible state; document it with an `// Impossible: …` comment.
//!
//! Tests are exempted because panicking in a `#[test]` is a clean
//! failure mode. Same exemption logic as `rust-no-unwrap`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};
use crate::rules::walker::walk_tree;

const BANNED_MACROS: &[&str] = &["panic", "todo", "unimplemented", "unreachable"];

#[derive(Debug)]
pub struct Check;

impl AstCheck for Check {
    fn check(&self, ctx: &CheckCtx, tree: &tree_sitter::Tree) -> Vec<Diagnostic> {
        let source_bytes = ctx.source.as_bytes();
        let mut diagnostics = Vec::new();
        walk_tree(tree, |node| {
            if node.kind() != "macro_invocation" {
                return;
            }
            let Some(macro_name_node) = node.child_by_field_name("macro") else {
                return;
            };
            let Ok(macro_name) = macro_name_node.utf8_text(source_bytes) else {
                return;
            };
            if !BANNED_MACROS.contains(&macro_name) {
                return;
            }
            if is_in_test_context(node, source_bytes) {
                return;
            }
            let pos = node.start_position();
            diagnostics.push(Diagnostic {
                path: ctx.path.to_path_buf(),
                line: pos.row + 1,
                column: pos.column + 1,
                rule_id: "rust-no-panic-macros".into(),
                message: format!(
                    "`{macro_name}!` aborts at runtime. Replace with a typed \
                     `Result` error. `todo!`/`unimplemented!` are placeholders \
                     that must not ship; `unreachable!` is only for \
                     compiler-proven impossible states with an `// Impossible:` \
                     comment. Tests are exempted."
                ),
                severity: Severity::Error,
            });
        });
        diagnostics
    }
}

/// Walks up parents looking for a `#[test]` fn or `#[cfg(test)]` mod.
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
    fn flags_panic_macro() {
        assert_eq!(run_on(r#"fn f() { panic!("boom"); }"#).len(), 1);
    }

    #[test]
    fn flags_todo_macro() {
        assert_eq!(run_on("fn f() { todo!(); }").len(), 1);
    }

    #[test]
    fn flags_unimplemented_macro() {
        assert_eq!(run_on("fn f() { unimplemented!(); }").len(), 1);
    }

    #[test]
    fn flags_unreachable_macro() {
        assert_eq!(run_on("fn f() { unreachable!(); }").len(), 1);
    }

    #[test]
    fn allows_panic_in_test_fn() {
        let source = "#[test]\nfn it_panics() { panic!(); }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_panic_in_cfg_test_module() {
        let source = "#[cfg(test)]\nmod tests { fn helper() { panic!(); } }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn does_not_flag_println() {
        assert!(run_on(r#"fn f() { println!("hi"); }"#).is_empty());
    }
}
