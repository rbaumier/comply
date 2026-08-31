use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, OxcCheck};
use crate::rules::comment_blocks;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        let comments = comment_blocks::from_oxc(semantic, ctx.source);

        super::flagged_wraps(comments, ctx.source)
            .into_iter()
            .map(|flag| Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line: flag.line,
                column: flag.column,
                rule_id: super::META.id.into(),
                message: super::MESSAGE.into(),
                severity: Severity::Error,
                span: None,
            })
            .collect()
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
        crate::rules::test_helpers::run_oxc_check(self, src, path, project, file)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, s, "t.ts")
    }

    #[test]
    fn flags_a_jsdoc_sentence_running_over_two_lines() {
        let src = r#"/**
 * Holds the connection settings of the endpoint and posts every non-stream
 * completion through fetch.
 */
export function build() {}"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_one_sentence_per_line() {
        let src = r#"/**
 * Holds the connection settings.
 * Each call builds its own client.
 */
export function build() {}"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_a_wrapped_line_comment_block() {
        let src = "\
// The client opens a fresh connection per request and only needs a
// mutable field to stash the response headers.
const count = 1;";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn separate_notes_stack_without_flagging() {
        let src = "\
// why: the pool times out under load.
// gotcha: the retry loop hides the cause.
const count = 1;";
        assert!(run(src).is_empty());
    }
}
