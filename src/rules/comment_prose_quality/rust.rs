//! comment-prose-quality — Rust backend.

use crate::diagnostic::Diagnostic;
use crate::rules::backend::{AstCheck, CheckCtx};
use crate::rules::walker::collect_nodes_of_kinds;

#[derive(Debug)]
pub struct Check;

impl AstCheck for Check {
    fn check(&self, ctx: &CheckCtx, tree: &tree_sitter::Tree) -> Vec<Diagnostic> {
        let nodes = collect_nodes_of_kinds(tree, &["line_comment", "block_comment"]);
        super::lint_comment_nodes(ctx, ctx.source.as_bytes(), &nodes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rust(source, &Check)
    }

    #[test]
    fn flags_weasel_word_in_line_comment() {
        let diags = run("// This is basically a wrapper.\nfn f() {}");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn allows_rust_doc_comments() {
        // `//!` markers should not trigger lexical illusion on `!`.
        let src = "//! Module doc.\n//!\n//! More details here.\nfn f() {}";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_rust_item_doc_comments() {
        let src = "/// Function doc.\n///\n/// More details.\nfn f() {}";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_rustdoc_heading_echo() {
        let src = "/// # Panics\n/// Panics if the buffer is empty.\nfn f() {}";
        assert!(!run(src).iter().any(|d| d.message.contains("Lexical illusion")));
        let src = "/// # Returns\n/// Returns `None` if not found.\nfn f() {}";
        assert!(!run(src).iter().any(|d| d.message.contains("Lexical illusion")));
        let src = "/// # Errors\n/// Errors if the input is invalid.\nfn f() {}";
        assert!(!run(src).iter().any(|d| d.message.contains("Lexical illusion")));
    }
}
