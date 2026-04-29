//! no-match-snapshot backend — flag `toMatchSnapshot()` / `toMatchInlineSnapshot()`.
//!
//! Why: snapshot tests are a maintenance trap. They capture the output
//! shape at one moment, then every unrelated refactor breaks them and
//! developers blindly update the snapshot. The test no longer asserts
//! anything specific — it asserts "whatever the code currently produces".
//! Assert on specific fields instead.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};

#[derive(Debug)]
pub struct Check;

impl AstCheck for Check {
    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["toMatchSnapshot", "toMatchInlineSnapshot"])
    }

    fn interested_kinds(&self) -> Option<&'static [&'static str]> {
        Some(&["call_expression"])
    }

    fn visit_node(
        &self,
        node: tree_sitter::Node,
        ctx: &CheckCtx,
        _state: Option<&mut dyn std::any::Any>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let source_bytes = ctx.source.as_bytes();
        let Some(function) = node.child_by_field_name("function") else {
            return;
        };
        if function.kind() != "member_expression" {
            return;
        }
        let Some(property) = function.child_by_field_name("property") else {
            return;
        };
        let Ok(method) = property.utf8_text(source_bytes) else {
            return;
        };
        if method != "toMatchSnapshot" && method != "toMatchInlineSnapshot" {
            return;
        }
        let pos = node.start_position();
        diagnostics.push(Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "no-match-snapshot".into(),
            message: format!(
                "`{method}()` is a maintenance trap — unrelated \
                 refactors break it and reviewers blindly update \
                 snapshots. Assert on specific fields instead."
            ),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    

    fn run_on(source: &str) -> Vec<Diagnostic> {


        crate::rules::test_helpers::run_ts(source, &Check)


    }

    #[test]
    fn flags_to_match_snapshot() {
        assert_eq!(run_on("expect(x).toMatchSnapshot();").len(), 1);
    }

    #[test]
    fn flags_to_match_inline_snapshot() {
        assert_eq!(run_on("expect(x).toMatchInlineSnapshot('y');").len(), 1);
    }

    #[test]
    fn allows_specific_assertions() {
        assert!(run_on("expect(x.foo).toBe('bar');").is_empty());
    }
}
