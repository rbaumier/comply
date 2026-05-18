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
mod tests {
    use super::*;

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(s, &Check)
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

    // Regression for #107: `// =>` trailers inside `@example` blocks are
    // source-as-prose and must not be counted as a long sentence.
    #[test]
    fn ignores_jsdoc_example_block_with_result_trailer() {
        let src = r#"/**
 * Atomically replace the child set of an N-N junction table for one parent.
 *
 * @example
 * const networks = yield* Result.await(replaceJunction({ a: 1 }));
 * // => [{ id: "n-1", name: "Pegas" }, { id: "n-2", name: "Cristal" }]
 */
export function replaceJunction() {}"#;
        assert!(run(src).is_empty(), "diagnostics: {:?}", run(src));
    }

    // Verify the `@example` skip ends when the next tag opens — long
    // sentences in subsequent prose still get flagged.
    #[test]
    fn still_flags_long_sentence_after_example_block() {
        let src = r#"/**
 * @example
 * const x = 1;
 * @remarks
 * this remark goes on and on and on and on and on and on and on and on and on and on forever please stop right now and ever.
 */
export function f() {}"#;
        assert_eq!(run(src).len(), 1);
    }
}
