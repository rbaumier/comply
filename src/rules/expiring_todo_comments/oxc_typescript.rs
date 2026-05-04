//! expiring-todo-comments oxc backend for TypeScript / JavaScript / TSX.

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
        let today = super::today_u32();
        let mut diagnostics = Vec::new();
        for comment in semantic.comments() {
            let start = comment.span.start as usize;
            let end = comment.span.end as usize;
            let Some(text) = ctx.source.get(start..end) else {
                continue;
            };
            if let Some(diag_msg) = super::check_comment_text(text, today) {
                let (line, column) = byte_offset_to_line_col(ctx.source, start);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: diag_msg,
                    severity: Severity::Warning,
                    span: None,
                });
            }
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
    fn flags_expired_todo() {
        let diags = run("// TODO [2020-01-01]: migrate to v2");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("2020-01-01"));
        assert!(diags[0].message.contains("expired"));
    }

    #[test]
    fn flags_expired_fixme() {
        let diags = run("// FIXME [2021-06-15]: remove workaround");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("FIXME"));
    }

    #[test]
    fn allows_future_date() {
        assert!(run("// TODO [2099-12-31]: future task").is_empty());
    }

    #[test]
    fn allows_todo_without_date() {
        assert!(run("// TODO fix this later").is_empty());
    }

    #[test]
    fn allows_todo_with_non_date_bracket() {
        assert!(run("// TODO [needs-review]: check this").is_empty());
    }

    #[test]
    fn flags_expired_in_block_comment() {
        let diags = run("/* TODO [2019-03-01]: old task */");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn ignores_code_not_comment() {
        assert!(run("const date = '2020-01-01';").is_empty());
    }
}
