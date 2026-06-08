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

#[cfg(test)]
mod tests {
    use super::*;



    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(s, &Check)
    }


    #[test]
    fn flags_camel_case_table_name() {
        let src = "const t = pgTable('orderItems', { id: serial('id') })";
        assert_eq!(run(src).len(), 1);
    }


    #[test]
    fn flags_singular_table_name() {
        // Use `profile` rather than `user` — `user` is now part of
        // the framework allowlist (better-auth) and is intentionally
        // accepted in singular form.
        let src = "const t = pgTable('profile', { id: serial('id') })";
        assert_eq!(run(src).len(), 1);
    }


    #[test]
    fn allows_snake_plural() {
        let src = "const t = pgTable('order_items', { id: serial('id') })";
        assert!(run(src).is_empty());
    }


    #[test]
    fn allows_simple_plural() {
        let src = "const t = pgTable('users', { id: serial('id') })";
        assert!(run(src).is_empty());
    }


    #[test]
    fn allows_better_auth_user_table() {
        // better-auth creates and queries the table named exactly
        // `user` — the singular form is imposed by the library, not
        // a styling choice the developer can change.
        let src = "const user = pgTable('user', { id: text('id').primaryKey() })";
        assert!(run(src).is_empty());
    }


    #[test]
    fn allows_better_auth_session_table() {
        let src = "const session = pgTable('session', { id: text('id').primaryKey() })";
        assert!(run(src).is_empty());
    }


    #[test]
    fn allows_better_auth_account_table() {
        let src = "const account = pgTable('account', { id: text('id').primaryKey() })";
        assert!(run(src).is_empty());
    }


    #[test]
    fn allows_better_auth_verification_table() {
        let src = "const verification = pgTable('verification', { id: text('id').primaryKey() })";
        assert!(run(src).is_empty());
    }


    #[test]
    fn allows_better_auth_organization_plugin_tables() {
        for name in ["organization", "member", "invitation"] {
            let src = format!("const t = pgTable('{name}', {{ id: text('id').primaryKey() }})");
            assert!(run(&src).is_empty(), "expected `{name}` to be allowed");
        }
    }


    #[test]
    fn allows_migration_tracking_tables() {
        for name in ["migration", "migrations", "schema_migrations"] {
            let src = format!("const t = pgTable('{name}', {{ id: serial('id') }})");
            assert!(run(&src).is_empty(), "expected `{name}` to be allowed");
        }
    }


    #[test]
    fn allows_better_auth_apikey_table() {
        let src = "const t = pgTable('apikey', { id: text('id').primaryKey() })";
        assert!(run(src).is_empty());
    }


    #[test]
    fn still_flags_non_allowlisted_singular_tables() {
        // Sanity: the allowlist is targeted — `profile`, `comment`,
        // etc. still get flagged for not being plural.
        let src = "const t = pgTable('profile', { id: serial('id') })";
        assert_eq!(run(src).len(), 1);
    }
}
