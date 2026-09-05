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
        for flag in super::flagged_sentences(comments, ctx.source, max) {
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
        let src = "/*!\nThis crate abstracts colored terminal text. Much of this API was motivated by use inside command line applications, where colors or styles can be configured by the end user and/or the environment.\n*/\nfn f() {}";
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
    fn a_rustdoc_colour_swatch_holds_no_words() {
        let src = "\
/// <style>.palette div{width:2rem;height:2rem}</style><div class=\"palette\" style=\"display:flex;flex-direction:row\"><div style=\"background-color: #f8fafc\"></div><div style=\"background-color: #f1f5f9\"></div><div style=\"background-color: #e2e8f0\"></div><div style=\"background-color: #cbd5e1\"></div><div style=\"background-color: #94a3b8\"></div></div>
const SLATE: u32 = 0;";
        assert!(run(src).is_empty());
    }

    #[test]
    fn a_long_prose_sentence_on_one_doc_line_still_counts() {
        let src = "\
/// This function walks the whole tree and then it walks it again and then a third time so that every node is visited.
fn walk() {}";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn a_fenced_sample_is_not_a_sentence() {
        let src = "\
/// Renders a chart.
///
/// ```text
/// x0 y0 x1 y1 x2 y2 x3 y3 x4 y4 x5 y5 x6 y6 x7 y7 x8 y8 x9 y9 xa ya xb yb xc yc xd yd
/// ```
fn chart() {}";
        assert!(run(src).is_empty());
    }

    #[test]
    fn prose_after_the_closing_fence_still_counts() {
        let src = "\
/// Builds it.
/// ```
/// let value = 1;
/// ```
/// This sentence after the fence runs on and on and on and on and never stops at all.
fn f() {}";
        let flags = run(src);
        assert_eq!(flags.len(), 1);
        assert_eq!(flags[0].line, 5);
    }
}
