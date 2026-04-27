//! sql-no-varchar — flag `VARCHAR(N)` / `CHAR(N)` column types in DDL.
//!
//! ## Why this rule was rewritten
//!
//! The original `TextCheck` scanned every line for the substring
//! `VARCHAR(` or `CHAR(`. That fired on any identifier whose name
//! ended in `_char(`, including the user's reported FP:
//!
//! ```ignore
//! fn flags_negative_lookahead_same_char() {
//!     assert_eq!(run(r#"const re = /(?!a)a/;"#).len(), 1);
//! }
//! ```
//!
//! Uppercased, the function name contains `SAME_CHAR(`, which has
//! `CHAR(` as a substring. The user disabled the rule in the
//! registry while we figured out the right approach.
//!
//! ## How the new rule works
//!
//! Two layered defenses:
//!
//! 1. **AstCheck on string literals only**, never on raw bytes of
//!    source code. Walks `string` / `template_string` (TS) and
//!    `string_literal` / `raw_string_literal` (Rust) nodes.
//! 2. **`sql_helpers::is_sql_ddl`** filters out everything that
//!    isn't a SQL DDL statement. Requires `CREATE` or `ALTER` AND
//!    `TABLE` or `TYPE`, both whole-word matched. VARCHAR/CHAR
//!    appear in column definitions, never in queries.
//! 3. **`sql_helpers::word_followed_by_open_paren`** checks that
//!    `varchar` / `char` appears at a word boundary AND is
//!    immediately followed by `(`. This catches `VARCHAR(255)` but
//!    not `same_char(`, `varchar_value`, etc.
//!
//! The combination is robust: the inner DDL filter rejects strings
//! that aren't schema, and the word-boundary check catches the
//! actual type usage.
//!
//! ## Language coverage
//!
//! - **TS / JS / TSX**, **Rust**, **Vue** (via `vue_sfc::extract_scripts`).

mod drizzle;
mod rust;
mod sql_text;
mod typescript;
mod vue;

#[cfg(test)]
mod shared_tests;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;
use crate::rules::sql_helpers::word_followed_by_open_paren;

pub const META: RuleMeta = RuleMeta {
    id: "sql-no-varchar",
    description: "`VARCHAR(N)` / `CHAR(N)` provides no perf benefit in PostgreSQL — use `TEXT` with a CHECK constraint.",
    remediation: "Replace `VARCHAR(N)` with `TEXT` + `CHECK(length(col) <= N)`. \
                  PostgreSQL has no length-based optimisation for VARCHAR; the \
                  N is enforced with the same trigger overhead as a CHECK.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["database", "sql"],
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (Language::TypeScript, Backend::TreeSitter(Box::new(typescript::Check))),
            (Language::TypeScript, Backend::TreeSitter(Box::new(drizzle::Check))),
            (Language::JavaScript, Backend::TreeSitter(Box::new(typescript::Check))),
            (Language::Tsx, Backend::TreeSitter(Box::new(typescript::Check))),
            (Language::Rust, Backend::TreeSitter(Box::new(rust::Check))),
            (Language::Vue, Backend::TreeSitter(Box::new(vue::Check))),
            (Language::Sql, Backend::Text(Box::new(sql_text::Check))),
        ],
    }
}

/// True if the (already-confirmed-as-DDL) SQL string `sql` declares
/// a `VARCHAR(N)` or `CHAR(N)` column. Both keyword and the open
/// paren are required so identifiers like `varchar_value` or
/// `same_char` don't trigger.
pub(super) fn sql_uses_varchar_or_char(sql: &str) -> bool {
    let lower = sql.to_ascii_lowercase();
    word_followed_by_open_paren(&lower, "varchar")
        || word_followed_by_open_paren(&lower, "char")
}

#[cfg(test)]
mod helper_tests {
    use super::*;

    #[test]
    fn flags_varchar_definition() {
        assert!(sql_uses_varchar_or_char("name VARCHAR(255) NOT NULL"));
    }

    #[test]
    fn flags_char_definition() {
        assert!(sql_uses_varchar_or_char("code CHAR(3)"));
    }

    #[test]
    fn flags_with_whitespace_before_paren() {
        assert!(sql_uses_varchar_or_char("name VARCHAR (255)"));
    }

    #[test]
    fn does_not_flag_text_column() {
        assert!(!sql_uses_varchar_or_char("name TEXT NOT NULL"));
    }

    #[test]
    fn does_not_flag_identifier_with_char_suffix() {
        assert!(!sql_uses_varchar_or_char(
            "fn flags_negative_lookahead_same_char()"
        ));
    }
}
