use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["line_comment", "block_comment"] => |node, source, ctx, diagnostics|
    let Ok(raw) = node.utf8_text(source) else { return };
    // Inner doc comments (`//!`, `/*!`) are crate-/module-level prose
    // documentation where full explanatory sentences are expected; the word
    // cap targets implementation notes, not docs.
    if raw.starts_with("//!") || raw.starts_with("/*!") { return; }
    let body = super::strip_markers(raw);
    if !super::has_long_sentence(&body) { return; }
    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        format!(
            "Comment sentence exceeds {} words. Split it — one idea per sentence.",
            super::MAX_WORDS_PER_SENTENCE
        ),
        Severity::Warning,
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
    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, s, "t.rs")
    }

    #[test]
    fn flags_long_sentence_rust() {
        let src = "// this comment goes on and on and on and on and on and on and on and on and on and on forever and ever and never stops\nfn f() {}";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_short_sentence_rust() {
        let src = "// short note\nfn f() {}";
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_long_block_comment_rust() {
        let src = "/* this comment goes on and on and on and on and on and on and on and on and on and on forever and ever and never stops here */\nfn f() {}";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_long_outer_doc_comment_rust() {
        let src = "/// this comment goes on and on and on and on and on and on and on and on and on and on forever and ever and never stops\nfn f() {}";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_inner_line_doc_comment_rust() {
        let src = "//! this module provides a cross platform abstraction for writing colored text to a terminal using either ANSI escape sequences or by communicating with a Windows console handle directly\nfn f() {}";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_inner_block_doc_comment_rust() {
        // Issue #6487: crate-level inner block doc (`/*!`) with a 26-word sentence.
        let src = "/*!\nThis crate provides a cross platform abstraction for writing colored text to a terminal. Much of this API was motivated by use inside command line applications, where colors or styles can be configured by the end user and/or the environment.\n*/\nfn f() {}";
        assert!(run(src).is_empty());
    }
}
