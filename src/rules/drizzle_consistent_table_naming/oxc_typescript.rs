use std::sync::Arc;

use oxc_ast::ast::Expression;

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};

const TABLE_CTORS: &[&str] = &["pgTable", "mysqlTable", "sqliteTable"];

const FRAMEWORK_TABLE_NAMES: &[&str] = &[
    "user",
    "session",
    "account",
    "verification",
    "organization",
    "member",
    "invitation",
    "apikey",
    "migration",
    "migrations",
    "schema_migrations",
];

fn is_snake_lower(s: &str) -> bool {
    !s.is_empty()
        && s.chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

fn looks_plural(s: &str) -> bool {
    let last = s.rsplit('_').next().unwrap_or(s);
    last.ends_with('s') || last.ends_with("data") || last.ends_with("info")
}

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
        let Expression::Identifier(ident) = &call.callee else {
            return;
        };
        if !TABLE_CTORS.contains(&ident.name.as_str()) {
            return;
        }
        let Some(first_arg) = call.arguments.first() else {
            return;
        };
        let Some(expr) = first_arg.as_expression() else {
            return;
        };
        let Expression::StringLiteral(lit) = expr else {
            return;
        };
        let table_name = lit.value.as_str();
        if FRAMEWORK_TABLE_NAMES.contains(&table_name) {
            return;
        }
        if is_snake_lower(table_name) && looks_plural(table_name) {
            return;
        }
        let (line, column) = byte_offset_to_line_col(ctx.source, lit.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "Table name `{table_name}` should be lowercase snake_case plural (e.g. `user_profiles`)."
            ),
            severity: Severity::Warning,
            span: None,
        });
    }
}
