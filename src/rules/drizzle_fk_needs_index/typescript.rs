//! drizzle-fk-needs-index — flag `.references()` without `.index()` in file.
//!
//! Walks the AST looking for call_expression nodes whose function is
//! a member_expression with property `references`. Then checks whether
//! the full source contains `.index(` — if not, flags it.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "call_expression" {
        return;
    }

    let Some(func) = node.child_by_field_name("function") else { return };
    if func.kind() != "member_expression" {
        return;
    }

    let Some(prop) = func.child_by_field_name("property") else { return };
    if prop.utf8_text(source).unwrap_or("") != "references" {
        return;
    }

    // Check if the file contains `.index(` anywhere.
    let full_source = ctx.source;
    if full_source.contains(".index(") {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "drizzle-fk-needs-index".into(),
        message: "FK `.references()` without `.index()` \
                  — PostgreSQL does NOT auto-index FK columns. \
                  Add an explicit index to avoid sequential scans \
                  on JOINs and cascading deletes."
            .into(),
        severity: Severity::Warning,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_fk_without_index() {
        assert_eq!(
            run_on("userId: integer('user_id').references(() => users.id)").len(),
            1
        );
    }

    #[test]
    fn allows_fk_with_index() {
        assert!(run_on(
            "userId: integer('user_id').references(() => users.id)\n  .index()"
        )
        .is_empty());
    }

    #[test]
    fn allows_no_references() {
        assert!(run_on("userId: integer('user_id')").is_empty());
    }
}
