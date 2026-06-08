//! no-section-divider-comments oxc backend for TypeScript / JavaScript / TSX.

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
        if ctx.file.path_segments.in_test_dir {
            return vec![];
        }
        let line_count = ctx.source.bytes().filter(|&b| b == b'\n').count() + 1;
        if line_count < 150 {
            return vec![];
        }
        let min_run = ctx
            .config
            .threshold("no-section-divider-comments", "min_run", ctx.lang);
        let mut diagnostics = Vec::new();
        for comment in semantic.comments() {
            let start = comment.span.start as usize;
            let end = comment.span.end as usize;
            let Some(text) = ctx.source.get(start..end) else {
                continue;
            };
            if !super::is_section_divider_text(text, min_run) {
                continue;
            }
            let (line, column) = byte_offset_to_line_col(ctx.source, start);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: "Section divider comment \u{2014} signal that the file is doing \
                     too many things. Split the file by responsibility instead \
                     of decorating the boundary with `===` or `***`."
                    .into(),
                severity: Severity::Error,
                span: None,
            });
        }
        if diagnostics.len() <= 1 {
            return vec![];
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
    use crate::rules::file_ctx::{FileCtx, PathSegments};

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    fn run_with_file_ctx(source: &str, file: &FileCtx) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule_with_ctx(&Check, source, "t.tsx", crate::project::default_static_project_ctx(), file)
    }

    fn large_file(extra: &str) -> String {
        let mut s = "const x = 1;\n".repeat(155);
        s.push_str(extra);
        s
    }

    #[test]
    fn flags_multiple_dividers_in_large_file() {
        let src = large_file("// ============\nconst y = 2;\n// ============\n");
        assert_eq!(run(&src).len(), 2);
    }

    #[test]
    fn flags_dashes_divider_in_large_file() {
        let src = large_file("// ----- SETUP -----\nconst y = 2;\n// ----- END -----\n");
        assert_eq!(run(&src).len(), 2);
    }

    #[test]
    fn allows_short_dashes() {
        assert!(run("// -- note").is_empty());
    }

    #[test]
    fn allows_normal_comment() {
        assert!(run("// Apply the cursor advance after commit").is_empty());
    }

    #[test]
    fn ignores_dividers_in_code() {
        assert!(run("const x = '====================';").is_empty());
    }

    #[test]
    fn flags_block_comment_divider_in_large_file() {
        let src = large_file("/* ============== */\nconst y = 2;\n/* ============== */\n");
        assert_eq!(run(&src).len(), 2);
    }

    #[test]
    fn allows_dividers_in_test_file() {
        let file = FileCtx {
            path_segments: PathSegments { in_test_dir: true, ..PathSegments::default() },
            ..FileCtx::default()
        };
        let src = large_file("// ============\nconst y = 2;\n// ============\n");
        assert!(run_with_file_ctx(&src, &file).is_empty());
    }

    #[test]
    fn allows_dividers_in_small_file() {
        assert!(run("// ============\nconst y = 2;\n// ============\n").is_empty());
    }

    #[test]
    fn allows_single_divider_in_large_file() {
        let src = large_file("// ============\nconst y = 2;\n");
        assert!(run(&src).is_empty());
    }

    #[test]
    fn still_flags_multiple_dividers_in_large_file() {
        let src = large_file("// ============\nconst y = 2;\n// ============\nconst z = 3;\n");
        assert!(!run(&src).is_empty());
    }
}
