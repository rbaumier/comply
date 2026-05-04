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
}
