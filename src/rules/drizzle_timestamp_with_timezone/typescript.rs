//! drizzle-timestamp-with-timezone backend — flag `timestamp('col')`
//! without `{ withTimezone: true }`.
//!
//! Why: bare `timestamp` columns are ambiguous across time zones. When
//! servers, clients, and databases live in different zones, `'2024-01-01
//! 12:00'` can mean three different points in time. `withTimezone: true`
//! stores an absolute instant and eliminates the ambiguity.
//!
//! Scope: PostgreSQL only. `{ withTimezone: true }` is a `pg-core` option;
//! `mysqlTable` / `sqliteTable` timestamps reject it (it is a TypeScript
//! type error there), so a `timestamp(...)` enclosed by one of those table
//! constructors is never flagged.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};

/// Non-PostgreSQL Drizzle table constructors. Their `timestamp` columns do
/// not accept `{ withTimezone: true }`, so the advice must not fire there.
const NON_PG_TABLE_CTORS: &[&str] = &["mysqlTable", "sqliteTable"];

/// Walks up from a `timestamp(...)` call to the nearest enclosing table
/// constructor and returns `true` when it is a non-PostgreSQL dialect
/// (`mysqlTable` / `sqliteTable`). A `pgTable` enclosure or no enclosing
/// table at all returns `false`, leaving the diagnostic active.
fn in_non_pg_table(node: tree_sitter::Node, source: &[u8]) -> bool {
    let mut current = node.parent();
    while let Some(ancestor) = current {
        if ancestor.kind() == "call_expression"
            && let Some(function) = ancestor.child_by_field_name("function")
            && function.kind() == "identifier"
            && let Ok(name) = function.utf8_text(source)
            && NON_PG_TABLE_CTORS.contains(&name)
        {
            return true;
        }
        current = ancestor.parent();
    }
    false
}

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
        // `withTimezone` is a `pg-core` option; skip `mysqlTable` /
        // `sqliteTable` columns, where it is not a valid Drizzle option.
        if in_non_pg_table(node, source_bytes) {
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
        crate::rules::test_helpers::run_ast_check(self, src, path, project, file)
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
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

    #[test]
    fn allows_mysql_table_timestamp() {
        // Issue #3313: `withTimezone` is pg-core-only; a mysqlTable timestamp
        // rejects it, so the advice must not fire here.
        let src = r#"
            import { mysqlTable, timestamp } from "drizzle-orm/mysql-core";
            export const users = mysqlTable("users", {
                createdAt: timestamp("created_at").notNull().defaultNow(),
            });
        "#;
        assert!(run_on(src).is_empty(), "{:?}", run_on(src));
    }

    #[test]
    fn allows_sqlite_table_timestamp() {
        let src = r#"
            import { sqliteTable, timestamp } from "drizzle-orm/sqlite-core";
            export const events = sqliteTable("events", {
                occurredAt: timestamp("occurred_at"),
            });
        "#;
        assert!(run_on(src).is_empty(), "{:?}", run_on(src));
    }

    #[test]
    fn still_flags_pg_table_timestamp_without_timezone() {
        // Guard: the diagnostic must remain active for PostgreSQL tables.
        let src = r#"
            import { pgTable, timestamp } from "drizzle-orm/pg-core";
            export const users = pgTable("users", {
                createdAt: timestamp("created_at"),
            });
        "#;
        assert_eq!(run_on(src).len(), 1, "{:?}", run_on(src));
    }

    #[test]
    fn allows_pg_table_timestamp_with_timezone() {
        // Guard: correct pg usage with the option present is clean.
        let src = r#"
            import { pgTable, timestamp } from "drizzle-orm/pg-core";
            export const users = pgTable("users", {
                createdAt: timestamp("created_at", { withTimezone: true }),
            });
        "#;
        assert!(run_on(src).is_empty(), "{:?}", run_on(src));
    }
}
