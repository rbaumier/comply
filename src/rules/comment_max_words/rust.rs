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
        for flag in super::flagged_sentences(comments, max) {
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
    fn flags_sentence_wrapped_over_several_lines() {
        let src = "\
/// Holds the connection settings and builds one client per call because the
/// underlying client opens a fresh connection per request anyway and only needs
/// a mutable borrow to stash the response headers it just read.
fn f() {}";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_long_inner_line_doc_comment_rust() {
        let src = "//! this module provides a cross platform abstraction for writing colored text to a terminal using either ANSI escape sequences or by communicating with a Windows console handle directly\nfn f() {}";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_long_inner_block_doc_comment_rust() {
        let src = "/*!\nThis crate provides a cross platform abstraction for writing colored text to a terminal. Much of this API was motivated by use inside command line applications, where colors or styles can be configured by the end user and/or the environment.\n*/\nfn f() {}";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_short_sentences_across_lines() {
        let src = "\
/// Holds the connection settings.
/// Each call builds its own client.
fn f() {}";
        assert!(run(src).is_empty());
    }

    #[test]
    fn fenced_code_in_a_doc_comment_counts_too() {
        let src = "\
/// Builds the client.
/// ```
/// let client = Client::new(endpoint, key, timeout, retries, headers, proxy, agent, pool, region, tenant, tracing, backoff, jitter, limits);
/// ```
fn f() {}";
        assert_eq!(run(src).len(), 1);
    }
}
