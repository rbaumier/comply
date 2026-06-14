//! Flag calls to `pgTable` / `mysqlTable` / `sqliteTable` whose first
//! string argument is not lowercase snake_case plural.

use crate::diagnostic::{Diagnostic, Severity};

const TABLE_CTORS: &[&str] = &["pgTable", "mysqlTable", "sqliteTable"];

/// Names that downstream frameworks pin by convention — the application
/// has no freedom to rename these without forking the framework, so the
/// pluralization rule does not apply.
///
/// - better-auth creates and queries `user`, `session`, `account`,
///   `verification`, plus `organization` / `member` / `invitation`
///   from its multi-tenant plugin. Renaming any of them breaks the
///   library's queries.
/// - `migration` / `migrations` / `schema_migrations` are migration
///   tracking tables produced by various tools (drizzle-kit, knex,
///   sqlx, …) and are referenced by exact name.
const FRAMEWORK_TABLE_NAMES: &[&str] = &[
    // better-auth core
    "user",
    "session",
    "account",
    "verification",
    // better-auth organization plugin
    "organization",
    "member",
    "invitation",
    // better-auth API key plugin
    "apikey",
    // migration tracking tables
    "migration",
    "migrations",
    "schema_migrations",
];

fn is_snake_lower(s: &str) -> bool {
    !s.is_empty()
        && s.chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

/// Heuristic plural check: ends with `s` (optionally `es`/`ies`) or with a
/// known uncountable/-en-style ending. Too strict a check causes FPs on
/// legitimate singular tables (e.g. `metadata`), so we accept any word
/// ending in `s` or any word containing `_` with last segment ending `s`.
fn looks_plural(s: &str) -> bool {
    let last = s.rsplit('_').next().unwrap_or(s);
    last.ends_with('s') || last.ends_with("data") || last.ends_with("info")
}

crate::ast_check! { on ["call_expression"] => |node, source, ctx, diagnostics|
    let Some(func) = node.child_by_field_name("function") else { return };
    if func.kind() != "identifier" {
        return;
    }
    let name = func.utf8_text(source).unwrap_or("");
    if !TABLE_CTORS.contains(&name) {
        return;
    }
    let Some(args) = node.child_by_field_name("arguments") else { return };
    let mut cursor = args.walk();
    let mut first_str: Option<tree_sitter::Node<'_>> = None;
    for c in args.children(&mut cursor) {
        if c.kind() == "string" {
            first_str = Some(c);
            break;
        }
    }
    let Some(s_node) = first_str else { return };
    let raw = s_node.utf8_text(source).unwrap_or("");
    let table_name = raw.trim_matches(['"', '\'']);
    if FRAMEWORK_TABLE_NAMES.contains(&table_name) {
        return;
    }
    if is_snake_lower(table_name) && looks_plural(table_name) {
        return;
    }
    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &s_node,
        super::META.id,
        format!(
            "Table name `{table_name}` should be lowercase snake_case plural (e.g. `user_profiles`)."
        ),
        Severity::Warning,
    ));
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

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, s, "t.ts")
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

    #[test]
    fn skips_scratch_schemas_in_test_dirs() {
        // Test files create scratch schemas with intentional short/generic
        // names for isolation. Table-naming consistency is a production-schema
        // concern, so the central gate must suppress the rule here.
        let src = "const t = pgTable('test', {});\nconst c = pgTable('cities1', {});";
        let diagnostics =
            crate::rules::test_helpers::run_rule_gated(&Check, src, "tests/pg-common.ts");
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn still_flags_singular_table_in_production_file() {
        // The rule is silenced only in test dirs, not globally: a production
        // schema with a singular, non-plural-snake_case name is still flagged.
        let src = "const t = pgTable('user_profile', {});";
        let diagnostics =
            crate::rules::test_helpers::run_rule_gated(&Check, src, "src/schema.ts");
        assert_eq!(diagnostics.len(), 1);
    }
}
