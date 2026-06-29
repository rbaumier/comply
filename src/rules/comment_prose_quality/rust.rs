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
impl crate::rules::test_helpers::RunRule for Check {
    fn meta(&self) -> &'static crate::rules::meta::RuleMeta {
        &super::META
    }
    fn execute_with_ctx(
        &self,
        src: &str,
        path: &std::path::Path,
        project: &crate::project::ProjectCtx,
        file: &crate::rules::file_ctx::FileCtx,
    ) -> Vec<crate::diagnostic::Diagnostic> {
        crate::rules::test_helpers::run_ast_check(self, src, path, project, file)
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.rs")
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
    fn allows_weasel_in_doc_comment() {
        let src = "/// Handles many concurrent connections.\nfn f() {}";
        assert!(run(src).iter().all(|d| !d.message.contains("Weasel")));
    }

    #[test]
    fn allows_passive_in_doc_comment() {
        let src = "/// The buffer is used to store data.\nfn f() {}";
        assert!(run(src).iter().all(|d| !d.message.contains("Passive")));
    }

    #[test]
    fn flags_weasel_in_inline_comment() {
        let src = "// This is basically a wrapper.\nfn f() {}";
        assert!(run(src).iter().any(|d| d.message.contains("Weasel")));
    }

    #[test]
    fn allows_doc_code_block() {
        let src = "/// ```\n/// }\n/// }\n/// ```\nfn f() {}";
        assert!(run(src).iter().all(|d| !d.message.contains("Lexical")));
    }

    // Regression for #6328: each `///` line is a separate tree-sitter node, so a
    // ```abnf fence opened on one node must keep the enclosed cross-line
    // repetition (`dotted-key` ... `dotted-key`) exempt on the following nodes.
    #[test]
    fn allows_abnf_fence_across_separate_doc_nodes() {
        let src = "/// ```abnf\n\
                   /// key = simple-key / dotted-key\n\
                   /// dotted-key = simple-key 1*( dot-sep simple-key )\n\
                   /// ```\nfn f() {}";
        assert!(
            !run(src)
                .iter()
                .any(|d| d.message.contains("Lexical illusion")),
            "{:?}",
            run(src)
        );
    }

    // A genuine doubled word across two adjacent prose `///` lines outside any
    // fence must still be flagged — proves the fence persistence didn't disable
    // the lexical-illusion check.
    #[test]
    fn flags_lexical_illusion_outside_fence() {
        let src = "/// This handles the\n/// the processing step.\nfn f() {}";
        let diags = run(src);
        assert_eq!(diags.len(), 1, "{diags:?}");
        assert!(diags[0].message.contains("Lexical illusion"));
    }

    // An unclosed fence in one doc block must not leak into a later,
    // non-contiguous block: the gap resets the fence so a real cross-line
    // illusion in the second block still flags.
    #[test]
    fn fence_state_resets_across_node_gap() {
        let src = "/// ```\n\
                   /// some code line\n\
                   fn a() {}\n\
                   \n\
                   /// This handles the\n\
                   /// the processing step.\nfn b() {}";
        let count = run(src)
            .iter()
            .filter(|d| d.message.contains("Lexical illusion"))
            .count();
        assert_eq!(count, 1);
    }

    #[test]
    fn allows_rustdoc_heading_echo() {
        let src = "/// # Panics\n/// Panics if the buffer is empty.\nfn f() {}";
        assert!(
            !run(src)
                .iter()
                .any(|d| d.message.contains("Lexical illusion"))
        );
        let src = "/// # Returns\n/// Returns `None` if not found.\nfn f() {}";
        assert!(
            !run(src)
                .iter()
                .any(|d| d.message.contains("Lexical illusion"))
        );
        let src = "/// # Errors\n/// Errors if the input is invalid.\nfn f() {}";
        assert!(
            !run(src)
                .iter()
                .any(|d| d.message.contains("Lexical illusion"))
        );
    }
}
