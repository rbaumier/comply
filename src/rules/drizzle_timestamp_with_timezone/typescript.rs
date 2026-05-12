//! drizzle-timestamp-with-timezone backend — flag `timestamp('col')`
//! without `{ withTimezone: true }`.
//!
//! Why: bare `timestamp` columns are ambiguous across time zones. When
//! servers, clients, and databases live in different zones, `'2024-01-01
//! 12:00'` can mean three different points in time. `withTimezone: true`
//! stores an absolute instant and eliminates the ambiguity.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};

#[derive(Debug)]
pub struct Check;

impl AstCheck for Check {
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
        let Ok(name) = function.utf8_text(source_bytes) else {
            return;
        };
        if name != "timestamp" {
            return;
        }
        let Some(args) = node.child_by_field_name("arguments") else {
            return;
        };
        let arg_count = args.named_child_count();
        // 2+ args: timestamp('col', { withTimezone: true }) — options present.
        if arg_count >= 2 {
            return;
        }
        // 1 arg: could be timestamp('col') OR timestamp({ withTimezone: true }).
        // If the single arg is an object, the user passed options directly
        // (Drizzle infers column name from the JS property key).
        if arg_count == 1 {
            if let Some(arg) = args.named_child(0) {
                if arg.kind() == "object" {
                    return;
                }
            }
        }
        let pos = node.start_position();
        diagnostics.push(Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "drizzle-timestamp-with-timezone".into(),
            message: "`timestamp('col')` without `{ withTimezone: true }` \
                      — ambiguous across time zones. Always use \
                      `timestamp('col', { withTimezone: true })`."
                .into(),
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
    fn flags_bare_timestamp() {
        assert_eq!(run_on("const t = timestamp('created_at');").len(), 1);
    }

    #[test]
    fn allows_timestamp_with_options() {
        assert!(run_on("const t = timestamp('created_at', { withTimezone: true });").is_empty());
    }

    #[test]
    fn allows_timestamp_options_without_column_name() {
        assert!(run_on("const t = timestamp({ withTimezone: true });").is_empty());
    }
}
