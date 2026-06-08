//! sql-no-offset-pagination — flag `LIMIT N OFFSET M` paginated queries.
//!
//! ## Why this rule was rewritten
//!
//! The original `TextCheck` flagged any line containing both `OFFSET`
//! and `LIMIT` as substrings. That fires on identifier lists like
//!
//! ```ignore
//! const AMBIGUOUS_BASES: &[&str] = &["…", "offset", "…", "limit", "…"];
//! ```
//!
//! where `"offset"` and `"limit"` are string literals in a Rust array,
//! not SQL keywords. The user disabled the rule in the registry while
//! we figured out the right approach.
//!
//! ## How the new rule works
//!
//! Detection is anchored at string literals via the SQL-rule
//! infrastructure built for #8 (`sql-no-between-timestamp`):
//!
//! 1. Walk the AST for string-literal nodes (`string` /
//!    `template_string` in TS, `string_literal` /
//!    `raw_string_literal` in Rust).
//! 2. `sql_helpers::is_sql_string` filters out everything that
//!    isn't a SQL query (whole-word DML keyword + clause keyword).
//! 3. `sql_uses_offset_pagination` checks the SQL for the
//!    co-occurrence of whole-word `OFFSET` and whole-word `LIMIT`.
//!    Both are required so that `OFFSET` in window-function clauses
//!    or other non-pagination contexts does not trigger.
//! 4. Diagnostic emitted at the string literal's start position.
//!
//! ## Language coverage
//!
//! - **TS / JS / TSX**: `typescript` backend.
//! - **Rust**: `rust` backend (handles both `"…"` and `r#"…"#`).
//! - **Vue**: `vue` backend, extracts `<script>` blocks via
//!   `vue_sfc::extract_scripts`, re-parses with the TS grammar, and
//!   reuses the same string-walk logic. Diagnostic line/column are
//!   translated back to Vue file coordinates.

mod oxc_drizzle;
#[cfg(test)]
mod drizzle;
mod rust;
mod sql_text;
mod oxc_typescript;
#[cfg(test)]
mod typescript;
mod vue;

#[cfg(test)]
mod shared_tests;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::sql_helpers::contains_word;

pub const META: RuleMeta = RuleMeta {
    id: "sql-no-offset-pagination",
    description: "`OFFSET` pagination is O(N) on deep pages — use cursor-based (keyset) pagination.",
    remediation: "Replace `LIMIT N OFFSET M` with cursor-based pagination: \
                  `WHERE id > :last_id ORDER BY id LIMIT N`. OFFSET scans \
                  and discards M rows every time.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["database", "sql"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: false,
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (
                Language::TypeScript,
                Backend::Oxc(Box::new(oxc_typescript::Check)),
            ),
            (
                Language::TypeScript,
                Backend::Oxc(Box::new(oxc_drizzle::Check)),
            ),
            (
                Language::JavaScript,
                Backend::Oxc(Box::new(oxc_typescript::Check)),
            ),
            (
                Language::Tsx,
                Backend::Oxc(Box::new(oxc_typescript::Check)),
            ),
            (Language::Rust, Backend::TreeSitter(Box::new(rust::Check))),
            (Language::Vue, Backend::TreeSitter(Box::new(vue::Check))),
            (Language::Sql, Backend::Text(Box::new(sql_text::Check))),
        ],
    }
}

/// True if the (already-confirmed) SQL string `sql` contains both
/// `LIMIT` and `OFFSET` as whole words. Both must be present so that
/// the detection only fires on actual paginated queries, not on
/// `OFFSET` inside window functions or other clauses.
pub(super) fn sql_uses_offset_pagination(sql: &str) -> bool {
    let lower = sql.to_ascii_lowercase();
    contains_word(&lower, "offset") && contains_word(&lower, "limit")
}

#[cfg(test)]
mod helper_tests {
    use super::*;

    #[test]
    fn flags_classic_pagination() {
        assert!(sql_uses_offset_pagination(
            "SELECT * FROM t LIMIT 10 OFFSET 100"
        ));
    }

    #[test]
    fn flags_lowercase_pagination() {
        assert!(sql_uses_offset_pagination(
            "select * from t limit ? offset ?"
        ));
    }

    #[test]
    fn does_not_flag_limit_only() {
        assert!(!sql_uses_offset_pagination("SELECT * FROM t LIMIT 10"));
    }

    #[test]
    fn does_not_flag_offset_only() {
        // OFFSET without LIMIT — rare in pagination, treat as not the smell.
        assert!(!sql_uses_offset_pagination("SELECT * FROM t OFFSET 100"));
    }

    #[test]
    fn does_not_flag_identifiers_containing_keywords() {
        // `offset_value` and `limit_value` are identifiers, not keywords.
        assert!(!sql_uses_offset_pagination(
            "offset_value = 1; limit_value = 2;"
        ));
    }
}
