//! sql-no-varchar — Drizzle ORM backend.
//!
//! Flags `varchar('col', { length: N })` calls in Drizzle schema
//! definitions. PostgreSQL has no length-based optimisation for
//! VARCHAR — `text()` plus a CHECK constraint is equivalent.

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
        if name != "varchar" {
            return;
        }
        let pos = node.start_position();
        diagnostics.push(Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: super::META.id.into(),
            message: "`varchar()` provides no benefit over `text()` in \
                      PostgreSQL — use `text()` with a CHECK constraint \
                      if you need length validation."
                .into(),
            severity: Severity::Error,
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
    fn flags_drizzle_varchar() {
        let src = "const name = varchar('name', { length: 255 });";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn does_not_flag_drizzle_text() {
        let src = "const name = text('name');";
        assert!(run(src).is_empty());
    }
}
