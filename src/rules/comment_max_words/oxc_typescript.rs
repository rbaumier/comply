use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for comment in semantic.comments().iter() {
            let start = comment.span.start as usize;
            let end = comment.span.end as usize;
            let Some(raw) = ctx.source.get(start..end) else {
                continue;
            };
            if raw.starts_with("/**") {
                continue;
            }
            let body = super::strip_markers(raw);
            if !super::has_long_sentence(&body) {
                continue;
            }
            let (line, column) = byte_offset_to_line_col(ctx.source, start);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: "comment-max-words".into(),
                message: format!(
                    "Comment sentence exceeds {} words. Split it — one idea per sentence.",
                    super::MAX_WORDS_PER_SENTENCE
                ),
                severity: Severity::Warning,
                span: None,
            });
        }
        diagnostics
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
        let src = "// this comment goes on and on and on and on and on and on and on and on and on and on forever please stop right now";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_short_sentence() {
        assert!(run("// short and sweet").is_empty());
    }

    #[test]
    fn allows_two_short_sentences() {
        assert!(run("// first thing happens. second thing happens.").is_empty());
    }

    // Regression for #460: JSDoc blocks (`/** */`) are entirely exempt from the
    // word limit — documentation comments legitimately need 30-50 words.
    #[test]
    fn allows_jsdoc_block_with_long_description() {
        let src = r#"/**
 * This JSDoc block explains the loader integration pattern in thorough detail,
 * covering the relationship between the preload mechanism and the form dialog
 * lifecycle across multiple rendering phases and async boundary contexts here.
 */
export function preloadFormDialog() {}"#;
        assert!(run(src).is_empty(), "diagnostics: {:?}", run(src));
    }

    // Regression for #460: JSDoc with @remarks containing a long sentence is also exempt.
    #[test]
    fn allows_jsdoc_block_with_long_remarks() {
        let src = r#"/**
 * @remarks
 * this remark goes on and on and on and on and on and on and on and on and on and on forever please stop right now and ever.
 */
export function f() {}"#;
        assert!(run(src).is_empty(), "diagnostics: {:?}", run(src));
    }
}
