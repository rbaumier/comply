use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};
use crate::rules::comment_blocks::{self, RawComment};
use std::sync::Arc;

pub struct Check;

type State = Vec<RawComment>;

impl AstCheck for Check {
    fn interested_kinds(&self) -> Option<&'static [&'static str]> {
        Some(&["line_comment", "block_comment"])
    }

    fn create_state(&self) -> Option<Box<dyn std::any::Any>> {
        Some(Box::new(State::new()))
    }

    fn visit_node(
        &self,
        node: tree_sitter::Node,
        ctx: &CheckCtx,
        state: Option<&mut dyn std::any::Any>,
        _diagnostics: &mut Vec<Diagnostic>,
    ) {
        let collected = state.unwrap().downcast_mut::<State>().unwrap();
        if let Some(comment) = comment_blocks::from_tree_sitter(&node, ctx.source) {
            collected.push(comment);
        }
    }

    fn finish(
        &self,
        ctx: &CheckCtx,
        state: Option<Box<dyn std::any::Any>>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let comments = *state.unwrap().downcast::<State>().unwrap();
        let max = ctx.config.threshold(super::META.id, "max", ctx.lang);
        for flag in super::flagged_blocks(comments, max) {
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line: flag.line,
                column: flag.column,
                rule_id: super::META.id.into(),
                message: super::message(flag.words, max),
                severity: Severity::Error,
                span: None,
            });
        }
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

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, s, "t.rs")
    }

    #[test]
    fn flags_long_line_comment_block() {
        let src = "\
// this is a long implementation note that keeps explaining the rationale in
// exhaustive detail across several full lines and easily runs past the fifty
// word budget because it just keeps going and going and going and going and
// going and never stops adding one more clause that could have lived in a
// dedicated doc comment or a shorter summary somewhere far more scannable here
fn f() {}";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_short_block() {
        assert!(run("// short note\n// second short line\nfn f() {}").is_empty());
    }

    #[test]
    fn outer_doc_comment_block_counts_too() {
        let src = "\
/// This documents the public API in full prose across several lines and words,
/// legitimately explaining the contract, invariants, and edge cases at length,
/// which is exactly what a documentation comment is for and still budgeted here,
/// because a doc comment nobody reads to the end documents nothing at all today.
/// The budget applies to every comment the reader has to walk through in order.
fn f() {}";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn standalone_block_comment_counts_alone() {
        let src = "\
/* this block comment on its own runs well past the small budget configured for
   the test by packing more than a dozen words onto its several wrapped lines */
fn f() {}";
        // With the default budget (50) this stays under; assert it does not flag.
        assert!(run(src).is_empty());
    }
}
