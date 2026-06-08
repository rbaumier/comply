//! no-history-in-comments oxc backend.

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
        for comment in semantic.comments() {
            let start = comment.span.start as usize;
            let end = comment.span.end as usize;
            let Some(raw) = ctx.source.get(start..end) else {
                continue;
            };
            if !super::mentions_history(raw) {
                continue;
            }
            let (line, column) = byte_offset_to_line_col(ctx.source, start);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: "Comment narrates history (`was`, `previously`, `refactored`, `rewritten`). Describe current behaviour — history lives in git log.".into(),
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
    fn flags_previously_used() {
        assert_eq!(run("// previously used a Map here").len(), 1);
    }


    #[test]
    fn flags_rewritten() {
        assert_eq!(run("// rewritten in v3").len(), 1);
    }


    #[test]
    fn flags_was_replaced() {
        assert_eq!(run("// was replaced with a Set").len(), 1);
    }


    #[test]
    fn allows_neutral_comment() {
        assert!(run("// caches results for 5 minutes").is_empty());
    }


    #[test]
    fn allows_descriptive_was() {
        assert!(run("// check if the value was provided").is_empty());
    }


    #[test]
    fn allows_jsdoc_with_was() {
        assert!(run("/** Returns whether the item was found */").is_empty());
    }


    #[test]
    fn allows_be_rewritten_as_behaviour() {
        // Regression for issue #494: "be rewritten" describes expected behaviour
        // (a verb), not a past code change.
        assert!(run("// non-string — should not be rewritten or crash").is_empty());
        assert!(run("// the URL will be rewritten to strip query params").is_empty());
    }
}
