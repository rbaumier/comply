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

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
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

    // Regression for #266: a weasel word inside a rule id cited by a
    // `comply-ignore-file` directive must not trip the prose linter.
    #[test]
    fn skips_comply_ignore_directive() {
        let src = "// comply-ignore-file: too-many-break-or-continue — each continue logs a skip reason.";
        assert!(run(src).is_empty(), "{:?}", run(src));
    }

    #[test]
    fn skips_comply_ignore_inline_directive() {
        assert!(run("// comply-ignore: various-rule — many cases handled here").is_empty());
    }

    // Regression for #4852: a JSDoc `@param {Type} name` tag line followed by a
    // description that begins with the capitalized param name is the JSDoc
    // grammar (name → description), not a doubled-word typo.
    #[test]
    fn allows_jsdoc_param_name_echo() {
        let src = "/**\n * Serialize an element node.\n *\n * @param {Element} node\n *   Node to handle.\n * @param {number | undefined} index\n *   Index of the node.\n * @param {Parents | undefined} parent\n *   Parent of the node.\n */\nfunction f() {}";
        assert!(
            !run(src)
                .iter()
                .any(|d| d.message.contains("Lexical illusion")),
            "{:?}",
            run(src)
        );
    }

    // A genuine doubled word in a JSDoc description must still be flagged.
    #[test]
    fn flags_lexical_illusion_in_jsdoc_description() {
        let src = "/**\n * @param {Element} node\n *   Node to handle the\n *   the processing step.\n */\nfunction f() {}";
        assert!(
            run(src)
                .iter()
                .any(|d| d.message.contains("Lexical illusion")),
            "{:?}",
            run(src)
        );
    }
}
