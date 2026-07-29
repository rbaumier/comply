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
        let max = ctx.config.threshold(super::META.id, "max", ctx.lang);

        let mut comments = Vec::new();
        for comment in semantic.comments().iter() {
            let start = comment.span.start as usize;
            let end = comment.span.end as usize;
            let Some(raw) = ctx.source.get(start..end) else {
                continue;
            };
            let (line, column) = byte_offset_to_line_col(ctx.source, start);
            comments.push(super::RawComment {
                start_byte: start,
                line,
                column,
                raw: raw.to_string(),
                is_line: raw.trim_start().starts_with("//"),
            });
        }

        super::flagged_blocks(comments, max)
            .into_iter()
            .map(|flag| Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line: flag.line,
                column: flag.column,
                rule_id: super::META.id.into(),
                message: format!(
                    "Comment block spans {} words (max {max}). Split it or move the detail into a doc comment.",
                    flag.words
                ),
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
    fn flags_long_line_comment_block() {
        let src = "\
// this is a long implementation note that keeps explaining the rationale in
// exhaustive detail across several full lines and easily runs past the fifty
// word budget because it just keeps going and going and going and going and
// going and never stops adding one more clause that could have lived in a
// dedicated doc comment or a shorter summary somewhere far more scannable here
fn_placeholder();";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_short_block() {
        assert!(run("// short note\n// second short line\nconst x = 1;").is_empty());
    }

    #[test]
    fn jsdoc_block_is_exempt() {
        let src = r#"/**
 * This JSDoc block explains the loader integration pattern in thorough detail,
 * covering the relationship between the preload mechanism and the form dialog
 * lifecycle across many rendering phases and async boundary contexts here and
 * there and everywhere well past any reasonable fifty word inline note budget.
 */
export function f() {}"#;
        assert!(run(src).is_empty());
    }
}
