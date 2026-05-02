//! comment-prose-quality oxc backend for TS / JS / TSX.

use crate::diagnostic::Diagnostic;
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{CheckCtx, OxcCheck};

pub struct Check;

impl OxcCheck for Check {
    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        let spans: Vec<(&str, usize)> = semantic
            .comments()
            .iter()
            .filter_map(|comment| {
                let start = comment.span.start as usize;
                let end = comment.span.end as usize;
                let raw = ctx.source.get(start..end)?;
                let (line_1based, _) = byte_offset_to_line_col(ctx.source, start);
                Some((raw, line_1based - 1))
            })
            .collect();
        super::lint_comment_spans(ctx, &spans)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
    }

    #[test]
    fn flags_weasel_word() {
        let diags = run("// This is basically a wrapper.");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("basically"));
    }

    #[test]
    fn flags_passive_voice() {
        let diags = run("// The value is used to compute the result.");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("is used"));
    }

    #[test]
    fn flags_lexical_illusion() {
        let src = "// This handles the\n// the processing step.";
        let diags = run(src);
        assert!(diags.iter().any(|d| d.message.contains("Lexical illusion")));
    }

    #[test]
    fn allows_clean_comments() {
        assert!(run("// Compute the sum of all items.").is_empty());
    }

    #[test]
    fn skips_doc_comments_weasel() {
        assert!(run("/// This is basically a wrapper.").is_empty());
    }
}
