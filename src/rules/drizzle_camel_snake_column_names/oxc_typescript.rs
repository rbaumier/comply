//! drizzle-camel-snake-column-names OXC backend — flag when a camelCase TS
//! property has a non-snake_case column name argument.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, CheckCtx, OxcCheck};
use oxc_ast::ast::{Expression, ObjectPropertyKind};
use std::sync::Arc;

pub struct Check;

const COLUMN_CTORS: &[&str] = &[
    "varchar",
    "text",
    "integer",
    "bigint",
    "smallint",
    "serial",
    "bigserial",
    "boolean",
    "timestamp",
    "date",
    "time",
    "numeric",
    "decimal",
    "real",
    "doublePrecision",
    "uuid",
    "json",
    "jsonb",
    "char",
];

fn is_camel_case(s: &str) -> bool {
    if s.is_empty() {
        return false;
    }
    let first = s.as_bytes()[0];
    if !first.is_ascii_lowercase() {
        return false;
    }
    s.chars().all(|c| c.is_ascii_alphanumeric()) && s.chars().any(|c| c.is_ascii_uppercase())
}

fn is_snake_case_lower(s: &str) -> bool {
    if s.is_empty() {
        return false;
    }
    s.chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

/// Descend through chained member calls (e.g. `varchar('x').notNull()`)
/// to find the base call and return its callee name.
fn base_call_name<'a>(expr: &'a Expression<'a>) -> Option<&'a str> {
    let mut cur = expr;
    loop {
        match cur {
            Expression::CallExpression(call) => match &call.callee {
                Expression::Identifier(ident) => return Some(ident.name.as_str()),
                Expression::StaticMemberExpression(member) => {
                    cur = &member.object;
                }
                _ => return None,
            },
            _ => return None,
        }
    }
}

/// Extract the first string literal argument from the base call in a chain.
fn first_string_arg<'a>(expr: &'a Expression<'a>) -> Option<&'a str> {
    let mut cur = expr;
    loop {
        match cur {
            Expression::CallExpression(call) => match &call.callee {
                Expression::Identifier(_) => {
                    // This is the base call.
                    for arg in &call.arguments {
                        if let Some(Expression::StringLiteral(s)) = arg.as_expression() {
                            return Some(s.value.as_str());
                        }
                    }
                    return None;
                }
                Expression::StaticMemberExpression(member) => {
                    cur = &member.object;
                }
                _ => return None,
            },
            _ => return None,
        }
    }
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [oxc_ast::AstType] {
        &[]
    }

    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();

        for node in semantic.nodes().iter() {
            let AstKind::ObjectExpression(obj) = node.kind() else {
                continue;
            };

            for prop in &obj.properties {
                let ObjectPropertyKind::ObjectProperty(p) = prop else {
                    continue;
                };

                let key_name = match &p.key {
                    oxc_ast::ast::PropertyKey::StaticIdentifier(id) => id.name.as_str(),
                    _ => continue,
                };

                if !is_camel_case(key_name) {
                    continue;
                }

                let Expression::CallExpression(_) = &p.value else {
                    continue;
                };

                let Some(ctor) = base_call_name(&p.value) else {
                    continue;
                };
                if !COLUMN_CTORS.contains(&ctor) {
                    continue;
                }

                let Some(col_name) = first_string_arg(&p.value) else {
                    continue;
                };
                if is_snake_case_lower(col_name) {
                    continue;
                }

                let span = p.span;
                let (line, column) = byte_offset_to_line_col(ctx.source, span.start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: format!(
                        "Property `{key_name}` is camelCase but its column name `{col_name}` is not snake_case — pass the snake_case database column name as the first argument."
                    ),
                    severity: Severity::Warning,
                    span: None,
                });
            }
        }

        diagnostics
    }
}
