//! banned-comment-words — TS/JS/TSX backend.
//!
//! Walks `comment` nodes from the TypeScript grammar and flags those
//! whose body contains a dismissive filler word at a word boundary.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["comment"] => |node, source, ctx, diagnostics|
    let Ok(text) = node.utf8_text(source) else { return; };
    let Some(word) = super::find_banned_word(text) else { return; };
    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        format!(
            "Comment uses `{word}` — dismissive filler that hides complexity. \
             Either explain the actual subtlety or delete the comment if the \
             line is genuinely self-explanatory."
        ),
        Severity::Error,
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_simply() {
        assert_eq!(run("// This simply works").len(), 1);
    }

    #[test]
    fn flags_obviously() {
        assert_eq!(run("// Obviously the cache wins").len(), 1);
    }

    #[test]
    fn flags_just() {
        assert_eq!(run("// just retry on failure").len(), 1);
    }

    #[test]
    fn allows_simplify() {
        assert!(run("// We simplify the input").is_empty());
    }

    #[test]
    fn allows_understanding() {
        assert!(run("// understanding the data flow").is_empty());
    }

    #[test]
    fn ignores_banned_word_in_code() {
        assert!(run("const obviously = true;").is_empty());
    }

    #[test]
    fn one_diagnostic_per_comment() {
        assert_eq!(run("// just simply works").len(), 1);
    }

    #[test]
    fn flags_block_comment() {
        assert_eq!(run("/* this is basically wrong */").len(), 1);
    }
}
