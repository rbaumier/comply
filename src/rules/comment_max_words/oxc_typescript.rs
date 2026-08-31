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
        let max = ctx.config.threshold(super::META.id, "max", ctx.lang);
        let comments = comment_blocks::from_oxc(semantic, ctx.source);

        super::flagged_sentences(comments, ctx.source, max)
            .into_iter()
            .map(|flag| Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line: flag.line,
                column: flag.column,
                rule_id: super::META.id.into(),
                message: super::message(flag.words, max),
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
    fn flags_long_sentence() {
        let src = "// this comment goes on and on and on and on and on and on and on and on and on and on forever and ever and never stops\nconst count = 1;";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_short_sentence() {
        assert!(run("// short note\nconst count = 1;").is_empty());
    }

    #[test]
    fn flags_sentence_wrapped_over_several_lines() {
        let src = "\
// Holds the connection settings and builds one client per call because the
// underlying client opens a fresh connection per request anyway and only needs
// a mutable field to stash the response headers it just read.
const count = 1;";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_long_jsdoc_sentence() {
        let src = r#"/**
 * This JSDoc block explains the loader integration pattern in thorough detail,
 * covering the relationship between the preload mechanism and the form dialog
 * lifecycle across many rendering phases and async boundary contexts.
 */
export function build() {}"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn jsdoc_example_bodies_count_too() {
        let src = r#"/**
 * Builds the client.
 * @example
 * const client = build(endpoint, key, timeout, retries, headers, proxy, agent, pool, extra, region, tenant, tracing, backoff, jitter, limits);
 */
export function build() {}"#;
        assert_eq!(run(src).len(), 1);
    }
}
