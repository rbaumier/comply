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
        for flag in super::flagged_wraps(comments) {
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line: flag.line,
                column: flag.column,
                rule_id: super::META.id.into(),
                message: super::MESSAGE.into(),
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
    fn flags_a_doc_sentence_running_over_two_lines() {
        let src = "\
/// Holds the connection settings of the Gemini endpoint and posts every
/// non-stream completion through reqwest.
fn f() {}";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_one_sentence_per_line() {
        let src = "\
/// Holds the connection settings.
/// Each call builds its own client.
fn f() {}";
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_a_wrapped_block_comment() {
        let src = "\
/* The client opens a fresh connection per request and only needs a
   mutable borrow to stash the response headers. */
fn f() {}";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn fenced_code_is_not_prose() {
        let src = "\
/// Builds the client.
/// ```
/// let client = Client::new(endpoint)
///     .with_timeout(timeout);
/// ```
fn f() {}";
        assert!(run(src).is_empty());
    }

    #[test]
    fn separate_notes_stack_without_flagging() {
        let src = "\
// why: the pool times out under load.
// gotcha: the retry loop hides the cause.
fn f() {}";
        assert!(run(src).is_empty());
    }
}
