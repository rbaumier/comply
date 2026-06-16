//! banned-comment-phrases — Rust backend.
//!
//! Walks `line_comment` and `block_comment` nodes and flags those whose body
//! contains an AI-tell narrator preamble or business-jargon phrase.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["line_comment", "block_comment"] => |node, source, ctx, diagnostics|
    let Ok(text) = node.utf8_text(source) else { return; };
    let Some(phrase) = super::find_banned_phrase(text) else { return; };
    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        format!(
            "Comment uses `{phrase}` — narrator filler typical of AI-generated \
             prose. State the point directly or delete the comment."
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

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.rs")
    }

    #[test]
    fn flags_line_comment() {
        assert_eq!(run("// here's the thing about ordering\nfn f() {}").len(), 1);
    }

    #[test]
    fn flags_block_comment() {
        assert_eq!(run("/* circle back to this once the API lands */\nfn f() {}").len(), 1);
    }

    #[test]
    fn allows_clean_comment() {
        assert!(run("// locks the mutex before the write\nfn f() {}").is_empty());
    }

    #[test]
    fn ignores_phrase_in_code() {
        assert!(run("fn deep_dive() {}").is_empty());
    }
}
