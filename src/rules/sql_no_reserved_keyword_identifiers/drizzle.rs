//! sql-no-reserved-keyword-identifiers — Drizzle ORM backend.
//!
//! Flags `pgTable('user', ...)` (table) and column-type calls like
//! `varchar('order')`, `text('group')`, `integer('table')` whose
//! first string argument is a PostgreSQL reserved keyword.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};

#[derive(Debug)]
pub struct Check;

const COLUMN_FNS: &[&str] = &[
    "text",
    "varchar",
    "integer",
    "boolean",
    "timestamp",
    "uuid",
    "serial",
    "bigint",
    "smallint",
    "numeric",
    "real",
    "doublePrecision",
    "json",
    "jsonb",
    "date",
    "time",
    "interval",
    "bigserial",
];

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
        let is_table = name == "pgTable";
        let is_column = COLUMN_FNS.contains(&name);
        if !is_table && !is_column {
            return;
        }
        let Some(args) = node.child_by_field_name("arguments") else {
            return;
        };
        let Some(first_arg) = args.named_child(0) else {
            return;
        };
        let Some(value) = string_literal_value(first_arg, source_bytes) else {
            return;
        };
        if !super::RESERVED.contains(&value.to_ascii_uppercase().as_str()) {
            return;
        }
        let pos = node.start_position();
        let message = if is_table {
            format!("`{value}` is a PostgreSQL reserved word — rename the table.")
        } else {
            format!("Column `{value}` is a PostgreSQL reserved word — rename it.")
        };
        diagnostics.push(Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: super::META.id.into(),
            message,
            severity: Severity::Warning,
            span: None,
        });
    }
}

fn string_literal_value(node: tree_sitter::Node, source: &[u8]) -> Option<String> {
    match node.kind() {
        "string" => {
            let mut cursor = node.walk();
            let mut out = String::new();
            for child in node.named_children(&mut cursor) {
                if child.kind() == "string_fragment" {
                    out.push_str(child.utf8_text(source).ok()?);
                }
            }
            Some(out)
        }
        "template_string" => {
            let mut cursor = node.walk();
            let mut out = String::new();
            for child in node.named_children(&mut cursor) {
                match child.kind() {
                    "string_fragment" => out.push_str(child.utf8_text(source).ok()?),
                    "template_substitution" => return None,
                    _ => {}
                }
            }
            Some(out)
        }
        _ => None,
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

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, src, "t.ts")
    }

    #[test]
    fn flags_pg_table_user() {
        let src = "const users = pgTable('user', { id: serial('id') });";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_varchar_order() {
        let src = "const col = varchar('order');";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_pg_table_users() {
        let src = "const users = pgTable('users', { id: serial('id') });";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_varchar_name() {
        let src = "const col = varchar('name');";
        assert!(run(src).is_empty());
    }
}
