//! banned-comment-words — Rust backend.
//!
//! Walks `line_comment` and `block_comment` nodes and flags those
//! whose body contains a dismissive filler word at a word boundary.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["line_comment", "block_comment"] => |node, source, ctx, diagnostics|
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
    fn flags_line_comment() {
        assert_eq!(run("// This simply works\nfn f() {}").len(), 1);
    }

    #[test]
    fn flags_block_comment() {
        assert_eq!(run("/* obviously fine */\nfn f() {}").len(), 1);
    }

    #[test]
    fn allows_simplify() {
        assert!(run("// We simplify the input\nfn f() {}").is_empty());
    }

    #[test]
    fn ignores_banned_word_in_code() {
        assert!(run("fn obviously_works() {}").is_empty());
    }

    #[test]
    fn flags_crucially() {
        assert_eq!(run("// crucially this runs first\nfn f() {}").len(), 1);
    }

    #[test]
    fn allows_actually_and_inherently() {
        // Both words are excluded as too false-positive-prone in code.
        assert!(run("// actually safe because inherently single-threaded\nfn f() {}").is_empty());
    }
}
