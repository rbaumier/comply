//! OxcCheck backend — flag `sql` tagged templates containing `IN (`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

/// Returns true if the text contains `IN (` followed (after optional whitespace) by `SELECT`.
fn in_followed_by_select(text: &str) -> bool {
    let upper = text.to_ascii_uppercase();
    for prefix in [" IN (", "\nIN (", "\tIN ("] {
        let mut search = upper.as_str();
        while let Some(pos) = search.find(prefix) {
            let after = search[pos + prefix.len()..].trim_start_matches([' ', '\t', '\n', '\r']);
            if after.starts_with("SELECT") {
                return true;
            }
            search = &search[pos + 1..];
        }
    }
    false
}

/// SQL clause keywords whose presence proves the `sql` template is more than a
/// bare `col IN (...)` predicate — a full or multi-clause query (#5750).
/// `inArray(col, [...])` builds a *standalone* predicate fragment from a column
/// object and a JS array; it can only replace the template when the template
/// *is* that predicate. It cannot be spliced into the middle of a hand-written
/// multi-line analytical / system-catalog query, a `sql.join(...)` fragment, or
/// any statement carrying other clauses, so those must not be flagged.
/// Markers are space-delimited and matched against a whitespace-collapsed,
/// space-padded uppercase rendering of the template (see `template_has_other_clauses`).
const NON_BARE_CLAUSE_MARKERS: &[&str] = &[
    " FROM ", " WHERE ", " JOIN ", " AND ", " OR ", " GROUP BY ", " ORDER BY ",
    " HAVING ", " UNION ", " INTERSECT ", " EXCEPT ",
];

/// True when the joined template text contains SQL beyond a single bare
/// `IN (...)` predicate. The quasis are joined, uppercased, whitespace-collapsed
/// and space-padded so a marker matches regardless of surrounding newlines/tabs
/// and even at the very start of the template (e.g. a fragment opening on `WHERE`).
fn template_has_other_clauses(joined: &str) -> bool {
    let normalized = joined.to_ascii_uppercase();
    let padded = format!(" {} ", normalized.split_whitespace().collect::<Vec<_>>().join(" "));
    NON_BARE_CLAUSE_MARKERS.iter().any(|marker| padded.contains(marker))
}

pub struct Check;

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

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, src, "t.ts")
    }

    #[test]
    fn flags_bare_in_predicate() {
        // The whole `sql` template is a bare `col IN (...)` predicate over a JS
        // array — `inArray(users.role, roles)` is the applicable rewrite, so the
        // true positive must still fire.
        let src = "db.select().from(users).where(sql`${users.role} IN (${roles})`)";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn ignores_full_query_with_literal_in() {
        // #5750 firing site (user-level-exclusivity.integration.test.ts): a raw
        // multi-clause analytical query. `inArray()` cannot be spliced into the
        // middle of it, so this must not be flagged.
        let src = "const q = sql`SELECT DISTINCT u.id FROM users u WHERE om.role IN ('org_admin', 'org_read')`";
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_sql_join_fragment_in_larger_query() {
        // #5750 firing site (trgm-indexes): a `NOT IN (${sql.join(...)})` clause
        // embedded in a larger raw query — already parameterized, inexpressible
        // via `inArray()`.
        let src = "const q = sql`SELECT relname FROM pg_class WHERE relname NOT IN (${notInList})`";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_in_subquery() {
        // inArray() does not support subqueries — false positive from #529.
        let src = r#"db.delete(account).where(sql`account.user_id IN (SELECT id FROM user WHERE email = ${email})`)"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_in_subquery_no_whitespace() {
        let src = r#"db.select().where(sql`id IN (SELECT id FROM user)`)"#;
        assert!(run(src).is_empty());
    }
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::TaggedTemplateExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::TaggedTemplateExpression(tagged) = node.kind() else { return };
        // Tag must be `sql`
        let Expression::Identifier(tag) = &tagged.tag else { return };
        if tag.name.as_str() != "sql" {
            return;
        }
        // Check quasis for `IN (` (case-insensitive)
        let has_in = tagged.quasi.quasis.iter().any(|q| {
            let upper = q.value.raw.to_ascii_uppercase();
            upper.contains(" IN (") || upper.contains("\nIN (") || upper.contains("\tIN (")
        });
        if !has_in {
            return;
        }
        // PL/pgSQL DO blocks use dollar-quoting (`DO $$` or `DO $tag$`).
        // inArray() cannot be used inside them, so skip.
        let is_do_block = tagged.quasi.quasis.iter().any(|q| {
            q.value.raw.to_ascii_uppercase().contains("DO $")
        });
        if is_do_block {
            return;
        }
        // `IN (SELECT ...)` is a subquery — inArray() does not support subqueries, skip.
        let has_in_subquery = tagged.quasi.quasis.iter().any(|q| {
            in_followed_by_select(&q.value.raw)
        });
        if has_in_subquery {
            return;
        }
        // The `IN` must be the *whole* predicate for `inArray()` to be an
        // applicable rewrite. A full or multi-clause query (FROM/WHERE/JOIN/
        // AND/OR/…) cannot host an `inArray()` fragment — skip it (#5750).
        let joined = tagged
            .quasi
            .quasis
            .iter()
            .map(|q| q.value.raw.as_str())
            .collect::<Vec<_>>()
            .join(" ");
        if template_has_other_clauses(&joined) {
            return;
        }
        let (line, column) = byte_offset_to_line_col(ctx.source, tagged.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`sql` template contains `IN (...)` \u{2014} prefer `inArray(col, [...])` for a parameterized, typed alternative.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}
