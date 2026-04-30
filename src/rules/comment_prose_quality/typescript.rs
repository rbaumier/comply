//! comment-prose-quality — TS/JS/TSX backend.
//!
//! Walks `comment` nodes in document order. The lexical-illusion check
//! kicks in when two consecutive comment nodes sit on adjacent source
//! lines, mimicking the original line-by-line behaviour.

use crate::diagnostic::Diagnostic;
use crate::rules::backend::{AstCheck, CheckCtx};
use crate::rules::walker::collect_nodes_of_kinds;

#[derive(Debug)]
pub struct Check;

impl AstCheck for Check {
    fn check(&self, ctx: &CheckCtx, tree: &tree_sitter::Tree) -> Vec<Diagnostic> {
        let nodes = collect_nodes_of_kinds(tree, &["comment"]);
        super::lint_comment_nodes(ctx, ctx.source.as_bytes(), &nodes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_weasel_word() {
        let diags = run("// This is basically a wrapper.");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("basically"));
    }

    #[test]
    fn flags_passive_voice() {
        let diags = run("// The value is used to compute the result.");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("is used"));
    }

    #[test]
    fn flags_lexical_illusion() {
        let src = "// This handles the\n// the processing step.";
        let diags = run(src);
        assert!(diags.iter().any(|d| d.message.contains("Lexical illusion")));
    }

    #[test]
    fn allows_clean_comment() {
        assert!(run("// Compute the SHA-256 hash of the input buffer.").is_empty());
    }

    #[test]
    fn ignores_word_in_code() {
        // Word outside a comment must not fire.
        assert!(run("const basically = 1;").is_empty());
    }

    #[test]
    fn ignores_punctuation_tokens_for_lexical_illusion() {
        let src = "// }\n// }";
        assert!(!run(src).iter().any(|d| d.message.contains("Lexical illusion")));
    }

    #[test]
    fn ignores_jsdoc_brace_for_lexical_illusion() {
        let src = "/**\n * }\n * }\n */";
        assert!(!run(src).iter().any(|d| d.message.contains("Lexical illusion")));
    }
}
