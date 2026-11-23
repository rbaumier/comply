use std::sync::OnceLock;

use regex::Regex;
use rustc_hash::FxHashSet;

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

/// One identifier. Quoted ones keep their case.
const IDENT: &str = r#"(?:"(?:[^"]|"")*"|[A-Za-z_][A-Za-z0-9_$]*)"#;

const REGEX_COMPILES: &str = "static regex compiles";

/// Captures the table a `CREATE TABLE` statement names.
fn create_table_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(&format!(
            r"(?i)^CREATE\s+(?:(?:GLOBAL|LOCAL|TEMP|TEMPORARY|UNLOGGED)\s+)*TABLE\s+(?:IF\s+NOT\s+EXISTS\s+)?({IDENT}(?:\s*\.\s*{IDENT})*)"
        ))
        .expect(REGEX_COMPILES)
    })
}

/// Captures the table a `CREATE [UNIQUE] INDEX` statement indexes.
fn create_index_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(&format!(
            r"(?i)^CREATE\s+(?:UNIQUE\s+)?INDEX\s+(?:IF\s+NOT\s+EXISTS\s+)?(?:{IDENT}\s+)?ON\s+(?:ONLY\s+)?({IDENT}(?:\s*\.\s*{IDENT})*)"
        ))
        .expect(REGEX_COMPILES)
    })
}

fn ident_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(IDENT).expect(REGEX_COMPILES))
}

/// `name` as Postgres resolves it, one entry per segment.
/// Bare segments fold to lowercase; quoted ones stay verbatim.
/// So `NEW . "t"` equals `new.t`, but `t` differs from `public.t`.
fn normalize_name(name: &str) -> Vec<String> {
    ident_re()
        .find_iter(name)
        .map(|m| {
            let segment = m.as_str();
            match segment.strip_prefix('"').and_then(|s| s.strip_suffix('"')) {
                Some(quoted) => quoted.replace("\"\"", "\""),
                None => segment.to_ascii_lowercase(),
            }
        })
        .collect()
}

/// The statement starting at `at` in `rest`, up to its `;`.
fn statement_at(rest: &str, at: usize) -> &str {
    let statement = &rest[at..];
    statement.find(';').map_or(statement, |end| &statement[..end])
}

/// True for a line that ends the current transaction.
fn ends_transaction(trimmed_upper: &str) -> bool {
    matches!(trimmed_upper, "COMMIT" | "COMMIT;" | "ROLLBACK" | "ROLLBACK;")
}

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    /// Spares an index on a table this transaction created.
    /// That table is empty and invisible to other sessions.
    /// Postgres refuses `CONCURRENTLY` inside the transaction anyway.
    /// squawk's `require-concurrent-index-creation` spares it too.
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        if !crate::rules::sql_helpers::is_migration_path(ctx.path) {
            return vec![];
        }
        let mut diags = Vec::new();
        let mut created_tables: FxHashSet<Vec<String>> = FxHashSet::default();
        let mut line_start: usize = 0;
        for (i, line) in ctx.source.split_inclusive('\n').enumerate() {
            let rest = &ctx.source[line_start..];
            line_start = line_start.saturating_add(line.len());
            let upper = line.to_ascii_uppercase();
            if ends_transaction(upper.trim()) {
                created_tables.clear();
            }
            let mut is_index_on_new_table = false;
            for (at, _) in upper.match_indices("CREATE") {
                let statement = statement_at(rest, at);
                if let Some(table) = create_table_re().captures(statement) {
                    created_tables.insert(normalize_name(&table[1]));
                } else if let Some(table) = create_index_re().captures(statement) {
                    is_index_on_new_table = created_tables.contains(&normalize_name(&table[1]));
                }
            }
            if upper.contains("CONCURRENTLY") || is_index_on_new_table {
                continue;
            }
            if upper.contains("CREATE INDEX") || upper.contains("CREATE UNIQUE INDEX") {
                diags.push(Diagnostic {
                    path: std::sync::Arc::clone(&ctx.path_arc),
                    line: i + 1,
                    column: 1,
                    rule_id: super::META.id.into(),
                    message: "`CREATE INDEX` without `CONCURRENTLY` locks the table. Use `CREATE INDEX CONCURRENTLY` instead.".into(),
                    severity: Severity::Error,
                    span: None,
                });
            }
        }
        diags
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn run(src: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(
            Path::new("/app/migrations/001.sql"),
            src,
        ))
    }

    fn run_non_migration(src: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("/app/src/schema.sql"), src))
    }

    #[test]
    fn flags_create_index() {
        assert_eq!(run("CREATE INDEX idx_email ON users(email);").len(), 1);
    }

    #[test]
    fn flags_create_unique_index() {
        assert_eq!(
            run("CREATE UNIQUE INDEX idx_ref ON orders(reference);").len(),
            1
        );
    }

    #[test]
    fn allows_concurrently() {
        assert!(run("CREATE INDEX CONCURRENTLY idx_email ON users(email);").is_empty());
    }

    #[test]
    fn flags_in_template_literal() {
        assert_eq!(run(r#"const q = `CREATE INDEX idx_x ON t(x)`;"#).len(), 1);
    }

    #[test]
    fn skips_non_migration_path() {
        assert!(run_non_migration("CREATE INDEX idx_email ON users(email);").is_empty());
    }

    #[test]
    fn allows_index_on_table_created_above() {
        let src = "SET lock_timeout = '5s';\n\
                   CREATE TABLE mail_message (\n\
                   \x20   id BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,\n\
                   \x20   status TEXT NOT NULL,\n\
                   \x20   created_at TIMESTAMPTZ NOT NULL DEFAULT now()\n\
                   );\n\
                   -- Serves the queue cap count and each delivery pass.\n\
                   CREATE INDEX mail_message_queued ON mail_message (created_at) WHERE status = 'queued';\n";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_unique_index_on_qualified_quoted_table_created_above() {
        let src = "CREATE TABLE IF NOT EXISTS kubernetes.quota_alert (id INT);\n\
                   -- Serves the lookup of one alert by id.\n\
                   CREATE UNIQUE INDEX quota_alert_id\n\
                   \x20   ON \"kubernetes\" . \"quota_alert\" (id);\n\
                   -- Serves the same lookup, spelled in mixed case.\n\
                   CREATE INDEX ON Kubernetes.Quota_Alert (id);\n";
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_index_on_table_not_created_in_file() {
        let src = "CREATE TABLE mail_message (id INT);\n\
                   -- Serves the login lookup by e-mail address.\n\
                   CREATE INDEX users_email ON users (email);\n";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_index_before_its_create_table() {
        let src = "-- Serves the lookup of one message by id.\n\
                   CREATE INDEX mail_message_id ON mail_message (id);\n\
                   CREATE TABLE mail_message (id INT);\n";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_index_on_same_name_in_another_schema() {
        let src = "CREATE TABLE kubernetes.quota_alert (id INT);\n\
                   -- Serves the lookup of one alert by id.\n\
                   CREATE INDEX quota_alert_id ON public.quota_alert (id);\n\
                   -- Serves the same lookup through search_path.\n\
                   CREATE INDEX quota_alert_bare ON quota_alert (id);\n";
        assert_eq!(run(src).len(), 2);
    }

    #[test]
    fn flags_index_on_table_created_before_commit() {
        let src = "BEGIN;\nCREATE TABLE t (id INT);\nCOMMIT;\n\
                   -- Serves the lookup of one row by id.\n\
                   CREATE INDEX t_id ON t (id);\n";
        assert_eq!(run(src).len(), 1);
    }
}
