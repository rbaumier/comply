//! sql-no-function-on-indexed-column — Drizzle ORM backend.
//!
//! Flags Drizzle's `lower(col)` / `upper(col)` SQL helper calls. Wrapping
//! a column in a function inside a WHERE clause prevents the planner
//! from using a plain B-tree index (the index is on the raw column, not
//! the transformed value). Either store the normalised form on write or
//! add a functional/expression index.

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
        if name.contains('.') {
            return;
        }
        if name != "lower" && name != "upper" {
            return;
        }
        let pos = node.start_position();
        diagnostics.push(Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: super::META.id.into(),
            message: "Wrapping a column in `lower()`/`upper()` kills index \
                      usage — store the normalized form or add a functional \
                      index."
                .into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(src, &Check)
    }

    #[test]
    fn flags_lower_in_eq() {
        let src = "where(eq(lower(users.email), 'john@example.com'));";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_upper_in_eq() {
        let src = "where(eq(upper(users.name), 'JOHN'));";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_text_column_decl() {
        let src = "const name = text('name');";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_eq_without_function() {
        let src = "where(eq(users.email, 'john@example.com'));";
        assert!(run(src).is_empty());
    }
}
