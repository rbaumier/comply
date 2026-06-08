use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Argument, Expression};
use std::sync::Arc;

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

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else {
            return;
        };
        let Expression::Identifier(id) = &call.callee else {
            return;
        };
        let name = id.name.as_str();
        let is_table = name == "pgTable";
        let is_column = COLUMN_FNS.contains(&name);
        if !is_table && !is_column {
            return;
        }
        let Some(first) = call.arguments.first() else {
            return;
        };
        let value = match first {
            Argument::StringLiteral(lit) => lit.value.as_str(),
            _ => return,
        };
        if !super::RESERVED.contains(&value.to_ascii_uppercase().as_str()) {
            return;
        }
        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        let message = if is_table {
            format!("`{value}` is a PostgreSQL reserved word — rename the table.")
        } else {
            format!("Column `{value}` is a PostgreSQL reserved word — rename it.")
        };
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message,
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
        crate::rules::test_helpers::run_oxc_check(self, src, path, project, file)
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, src, "t.ts")
    }

    #[test]
    fn flags_pg_table_user() {
        assert_eq!(run_on("const users = pgTable('user', { id: serial('id') });").len(), 1);
    }

    #[test]
    fn flags_varchar_order() {
        assert_eq!(run_on("const col = varchar('order');").len(), 1);
    }

    #[test]
    fn allows_pg_table_users() {
        assert!(run_on("const users = pgTable('users', { id: serial('id') });").is_empty());
    }

    #[test]
    fn allows_varchar_name() {
        assert!(run_on("const col = varchar('name');").is_empty());
    }
}
