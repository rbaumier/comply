//! rust-unsafe-impl-without-comment backend.
//!
//! Walks `impl_item` nodes and flags any whose source text starts
//! with `unsafe impl` and that has no `// SAFETY:` comment on the
//! lines directly above. Same scan-upward logic as
//! `rust-undocumented-unsafe`: skip blanks and other comments, stop
//! at the first real code line.

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
            if node.kind() != "impl_item" {
                return;
            }
            // Check the impl's source prefix for the `unsafe` keyword.
            // tree-sitter-rust doesn't expose `unsafe` as a named child
            // on impl_item, so we read the first chunk of the node's text.
            let Ok(text) = node.utf8_text(source_bytes) else {
                return;
            };
            if !text.trim_start().starts_with("unsafe impl") {
                return;
            }
            if has_safety_comment_above(node, ctx.source) {
                return;
            }
            let pos = node.start_position();
            diagnostics.push(Diagnostic {
                path: ctx.path.to_path_buf(),
                line: pos.row + 1,
                column: pos.column + 1,
                rule_id: "rust-unsafe-impl-without-comment".into(),
                message: "`unsafe impl` without a `// SAFETY:` comment — \
                          spell out which invariants of the unsafe trait \
                          the type upholds. The comment is the entire \
                          audit trail for the unsafe contract."
                    .into(),
                severity: Severity::Error,
            });
        });
        diagnostics
    }
}

fn has_safety_comment_above(node: tree_sitter::Node, source: &str) -> bool {
    let start_row = node.start_position().row;
    if start_row == 0 {
        return false;
    }
    let lines: Vec<&str> = source.lines().collect();
    let mut row = start_row;
    while row > 0 {
        row -= 1;
        let Some(line) = lines.get(row) else { break };
        let trimmed = line.trim_start();
        if trimmed.is_empty() {
            continue;
        }
        if trimmed.starts_with("//") || trimmed.starts_with("/*") {
            if trimmed.contains("SAFETY:") || trimmed.contains("Safety:") {
                return true;
            }
            continue;
        }
        break;
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
    fn flags_bare_unsafe_impl_send() {
        let source = "struct Foo;\nunsafe impl Send for Foo {}";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn allows_unsafe_impl_with_safety_comment() {
        let source = "struct Foo;\n\
                      // SAFETY: Foo holds only Send fields, so the type is itself Send.\n\
                      unsafe impl Send for Foo {}";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn does_not_flag_safe_impl() {
        let source = "struct Foo;\nimpl Display for Foo { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { Ok(()) } }";
        assert!(run_on(source).is_empty());
    }
}
