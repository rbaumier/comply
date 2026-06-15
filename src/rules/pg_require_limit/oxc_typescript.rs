//! pg-require-limit OXC backend.
//!
//! Flags SQL `SELECT` queries without `LIMIT` in string/template literals.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use crate::rules::sql_helpers::{contains_word, is_sql_string};
use oxc_ast::ast::Expression;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::StringLiteral, AstType::TemplateLiteral]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let (text, span_start, span_len) = match node.kind() {
            AstKind::StringLiteral(lit) => {
                (lit.value.as_str().to_string(), lit.span.start, (lit.span.end - lit.span.start) as usize)
            }
            AstKind::TemplateLiteral(tpl) => {
                // Concatenate quasis, replacing expressions with spaces
                let mut out = String::new();
                for (i, quasi) in tpl.quasis.iter().enumerate() {
                    out.push_str(quasi.value.raw.as_str());
                    if i < tpl.quasis.len() - 1 {
                        out.push(' ');
                    }
                }
                (out, tpl.span.start, (tpl.span.end - tpl.span.start) as usize)
            }
            _ => return,
        };

        if text.is_empty() {
            return;
        }
        if !is_sql_string(&text) {
            return;
        }
        if !starts_with_select(&text) {
            return;
        }
        let lower = text.to_ascii_lowercase();
        if contains_word(&lower, "limit") {
            return;
        }
        if is_implicitly_bounded(&lower) {
            return;
        }
        if is_plpgsql_select_into(&lower) {
            return;
        }
        if is_passed_to_data_sink(node, semantic) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, span_start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: "pg-require-limit".into(),
            message: "SQL `SELECT` without `LIMIT` can return an unbounded number of rows — \
                      add `LIMIT n` or a unique-row predicate (`WHERE id = ...`, `COUNT(..)`)."
                .into(),
            severity: Severity::Error,
            span: Some((span_start as usize, span_len)),
        });
    }
}

/// Method names that execute a SQL string against a database — the SQL passed
/// to one of these is a real query. Covers node-postgres / mysql2 (`query`),
/// better-sqlite3 / node:sqlite (`prepare`, `run`, `all`, `get`, `exec`),
/// generic drivers (`execute`), and Prisma's raw escape hatches (`$queryRaw`,
/// `$executeRaw`, and their `*Unsafe` variants).
const DB_EXEC_METHODS: &[&str] = &[
    "query",
    "execute",
    "exec",
    "run",
    "all",
    "get",
    "prepare",
    "$queryRaw",
    "$queryRawUnsafe",
    "$executeRaw",
    "$executeRawUnsafe",
];

/// True when the SQL literal is an argument to a *method* call whose name is
/// not a known database-execution method (`obj.setQueryResult(sql, …)`,
/// `spy.mockResolvedValueOnce(sql)`, `expect(captured).toBe(sql)`). Such a
/// string is query *data* — a mock-adapter lookup key, a canned mock result, or
/// an assertion expectation — not a query executed against a database, so
/// requiring a `LIMIT` on it is a false positive.
///
/// A standalone literal (a bare `const q = "SELECT …"`, an array/object element,
/// or an argument to a recognized DB-execution method or a tagged template) is
/// treated as a query and still flagged: the nearest enclosing call/tagged
/// template is inspected, and only a non-execution *method* call exempts.
fn is_passed_to_data_sink<'a>(
    node: &oxc_semantic::AstNode<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> bool {
    for ancestor in semantic.nodes().ancestors(node.id()) {
        match ancestor.kind() {
            // A tagged template (`sql`...``, `db.$queryRaw`...``) executes its
            // literal — not a data sink.
            AstKind::TaggedTemplateExpression(_) => return false,
            AstKind::CallExpression(call) => {
                let Expression::StaticMemberExpression(member) = &call.callee else {
                    // A plain function call (`foo(sql)`) is ambiguous; treat it
                    // as execution so genuine queries keep flagging.
                    return false;
                };
                return !DB_EXEC_METHODS.contains(&member.property.name.as_str());
            }
            _ => {}
        }
    }
    false
}

fn starts_with_select(text: &str) -> bool {
    let trimmed = text.trim_start();
    let head: String = trimmed
        .chars()
        .take(8)
        .collect::<String>()
        .to_ascii_lowercase();
    head.starts_with("select") || head.starts_with("with ") || head.starts_with("with\t")
}

fn is_implicitly_bounded(lower: &str) -> bool {
    let has_group_by = contains_phrase(lower, "group by");
    if !has_group_by {
        for agg in ["count(", "sum(", "avg(", "min(", "max("] {
            if lower.contains(agg) {
                return true;
            }
        }
    }

    if lower.contains("exists(") || lower.contains("exists (") {
        return true;
    }

    if contains_word(lower, "where") && has_id_equality(lower) {
        return true;
    }

    false
}

/// True for a PL/pgSQL `SELECT ... INTO <variable>` single-row assignment.
///
/// `SELECT col INTO var FROM ...` fetches at most one row into a PL/pgSQL
/// variable — it is semantically `LIMIT 1`, and adding `LIMIT` is redundant
/// or a syntax error. The SQL table-creation form (`SELECT ... INTO
/// [TEMP|TEMPORARY|UNLOGGED] [TABLE] new_table FROM ...`) is still an
/// unbounded query, so it is excluded by inspecting the token after `INTO`.
fn is_plpgsql_select_into(lower: &str) -> bool {
    let Some(after_into) = next_word_after(lower, "into") else {
        return false;
    };
    !matches!(after_into, "table" | "temp" | "temporary" | "unlogged")
}

/// Returns the whole-word token that immediately follows a whole-word
/// `keyword`, stripped of surrounding non-identifier characters.
fn next_word_after<'a>(lower: &'a str, keyword: &str) -> Option<&'a str> {
    let strip = |word: &'a str| word.trim_matches(|c: char| !c.is_ascii_alphanumeric() && c != '_');
    let mut words = lower.split_whitespace().map(strip);
    words
        .by_ref()
        .position(|word| word == keyword)
        .and_then(|_| words.next())
}

fn contains_phrase(lower: &str, phrase: &str) -> bool {
    lower
        .split_whitespace()
        .collect::<Vec<_>>()
        .windows(phrase.split_whitespace().count())
        .any(|window| window.join(" ") == phrase)
}

fn has_id_equality(lower: &str) -> bool {
    let bytes = lower.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'i'
            && i + 1 < bytes.len()
            && bytes[i + 1] == b'd'
            && (i + 2 == bytes.len() || !is_ident_byte(bytes[i + 2]))
            && (i == 0 || !is_ident_byte(bytes[i - 1]) || bytes[i - 1] == b'.')
        {
            let mut j = i + 2;
            while j < bytes.len() && bytes[j].is_ascii_whitespace() {
                j += 1;
            }
            if j < bytes.len() {
                if bytes[j] == b'=' {
                    return true;
                }
                if j + 1 < bytes.len()
                    && bytes[j] == b'i'
                    && bytes[j + 1] == b'n'
                    && (j + 2 == bytes.len() || !is_ident_byte(bytes[j + 2]))
                {
                    let mut k = j + 2;
                    while k < bytes.len() && bytes[k].is_ascii_whitespace() {
                        k += 1;
                    }
                    if k < bytes.len() && bytes[k] == b'(' {
                        return true;
                    }
                }
            }
        }
        i += 1;
    }
    false
}

fn is_ident_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

#[cfg(test)]
mod tests {
    fn run(source: &str) -> Vec<crate::diagnostic::Diagnostic> {
        crate::rules::test_helpers::run_rule_by_id("pg-require-limit", source, "t.ts")
    }

    #[test]
    fn flags_select_without_limit() {
        let source = r#"const q = "SELECT * FROM users WHERE active = true";"#;
        assert_eq!(run(source).len(), 1);
    }

    // Regression for #1790: a PL/pgSQL `SELECT col INTO var FROM ...` is a
    // single-row variable assignment (implicit `LIMIT 1`), not an unbounded
    // result set — the supabase pg-meta `SELECT conname INTO r` pattern.
    #[test]
    fn allows_plpgsql_select_into_variable() {
        let source = r#"const q = "SELECT conname INTO r FROM pg_constraint WHERE contype = 'p'";"#;
        assert!(run(source).is_empty());
    }

    // The explicit SQL table-creation form (`SELECT ... INTO TABLE new_table`)
    // is still an unbounded query and must remain flagged.
    #[test]
    fn flags_select_into_table_creation() {
        let source =
            r#"const q = "SELECT * INTO TABLE archived_users FROM users WHERE active = false";"#;
        assert_eq!(run(source).len(), 1);
    }

    // Regression for #3359: a SQL string passed as a mock-adapter lookup key
    // (prisma `bench/bench-utils.ts`) is query *data*, not an executed query —
    // `setQueryResult` registers a canned result for a query pattern.
    #[test]
    fn allows_sql_as_mock_adapter_query_key() {
        let source = r#"
            mockAdapter.setQueryResult('SELECT id, email, name FROM User WHERE id', {
              columnNames: ['id', 'email', 'name'],
              rows: [[1, 'user1@example.com', 'User 1']],
            });
        "#;
        assert!(run(source).is_empty());
    }

    // The SQL string passed to an assertion matcher or a mock-return setup is
    // also data, not an executed query.
    #[test]
    fn allows_sql_in_assertion_and_mock_return() {
        let assertion = r#"expect(captured).toBe('SELECT * FROM users WHERE active = true');"#;
        assert!(run(assertion).is_empty());
        let mock = r#"spy.mockResolvedValueOnce('SELECT * FROM users WHERE active = true');"#;
        assert!(run(mock).is_empty());
    }

    // Over-exemption guard: a SQL string executed via a database method
    // (`db.query(...)`) is a real query and must still be flagged.
    #[test]
    fn flags_executed_db_query() {
        let source = r#"const rows = db.query("SELECT * FROM users WHERE active = true");"#;
        assert_eq!(run(source).len(), 1);
    }

    // Over-exemption guard: a tagged-template query (`sql`...``) executes its
    // literal and must still be flagged.
    #[test]
    fn flags_tagged_template_query() {
        let source = r#"const rows = sql`SELECT * FROM users WHERE active = true`;"#;
        assert_eq!(run(source).len(), 1);
    }
}
