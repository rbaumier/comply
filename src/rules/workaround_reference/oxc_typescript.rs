use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["workaround", "hack", "Workaround", "Hack", "HACK"])
    }

    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        let lines: Vec<&str> = ctx.source.lines().collect();

        for comment in semantic.comments() {
            let start = comment.span.start as usize;
            let end = comment.span.end as usize;
            if end > ctx.source.len() {
                continue;
            }
            let text = &ctx.source[start..end];

            if !super::has_keyword(text) {
                continue;
            }
            if super::has_reference(text) {
                continue;
            }

            let (line, _) = byte_offset_to_line_col(ctx.source, start);
            let row = line.saturating_sub(1);
            let lookahead = (row + 1..=(row + 2).min(lines.len().saturating_sub(1)))
                .any(|i| super::has_reference(lines[i]));
            if lookahead {
                continue;
            }

            // Anchor on the comment itself: two `// hack` comments on one line
            // (e.g. after two statements) are two findings at two columns.
            // `start`/`end` are the comment's own byte range, so the span
            // highlights exactly the comment text.
            diagnostics.push(Diagnostic::at_offset(
                Arc::clone(&ctx.path_arc),
                ctx.source,
                (start, end - start),
                super::META.id,
                "Workaround/hack comment without an issue reference — \
                 add a link or ticket number."
                    .into(),
                Severity::Error,
            ));
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

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    #[test]
    fn anchors_each_comment_of_a_shared_line_on_its_own_column() {
        // Regression for rbaumier/comply#8386 — two unreferenced hack comments
        // on one line used to produce two records identical in every
        // serialized field.
        let src = "const a = 1; /* hack */ const b = 2; /* hack */";
        let diags = run_on(src);
        let positions: Vec<(usize, usize)> = diags.iter().map(|d| (d.line, d.column)).collect();
        assert_eq!(positions, vec![(1, 14), (1, 38)]);
        for d in &diags {
            let (offset, len) = d.span.expect("the anchor carries the comment's span");
            assert_eq!(&src[offset..offset + len], "/* hack */");
        }
    }
}
