//! no-abusive-eslint-disable oxc backend for TypeScript / JavaScript / TSX.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["eslint-disable"])
    }

    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for comment in semantic.comments() {
            let start = comment.span.start as usize;
            let end = comment.span.end as usize;
            let Some(text) = ctx.source.get(start..end) else {
                continue;
            };
            if !super::is_abusive_disable(text) {
                continue;
            }
            let (line, column) = byte_offset_to_line_col(ctx.source, start);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: "Specify the rules you want to disable.".into(),
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

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
    }

    #[test]
    fn flags_bare_disable_next_line() {
        assert_eq!(run("// eslint-disable-next-line\nconst x = 1;").len(), 1);
    }

    #[test]
    fn flags_bare_disable() {
        assert_eq!(run("/* eslint-disable */").len(), 1);
    }

    #[test]
    fn flags_bare_disable_line() {
        assert_eq!(run("const x = 1; // eslint-disable-line").len(), 1);
    }

    #[test]
    fn allows_specific_rule() {
        assert!(run("// eslint-disable-next-line no-console\nconst x = 1;").is_empty());
    }

    #[test]
    fn allows_specific_rule_in_block() {
        assert!(run("/* eslint-disable no-unused-vars */").is_empty());
    }

    #[test]
    fn allows_scoped_rule() {
        assert!(
            run("// eslint-disable-next-line @typescript-eslint/no-explicit-any\nconst x = 1;")
                .is_empty()
        );
    }

    #[test]
    fn flags_with_description_separator() {
        assert_eq!(
            run("// eslint-disable-next-line -- reason\nconst x = 1;").len(),
            1
        );
    }

    #[test]
    fn ignores_non_comment_lines() {
        assert!(run("const eslintDisable = true;").is_empty());
    }
}
