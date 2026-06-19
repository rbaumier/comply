//! sql-no-pg-enum

mod oxc_drizzle;
#[cfg(test)]
mod drizzle;
mod rust;
mod text;
mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "sql-no-pg-enum",
    description: "PostgreSQL `CREATE TYPE ... AS ENUM` — can't remove values once added.",
    remediation: "Replace PG enums with a CHECK constraint (`status TEXT CHECK(status IN ('a','b','c'))`) or a lookup table. PG enums can't have values removed — they're append-only, which makes rollbacks impossible.",
    severity: Severity::Error,
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
            (Language::Vue, Backend::Text(Box::new(text::Check))),
            (Language::Sql, Backend::Text(Box::new(text::Check))),
        ],
    }
}

/// True if `text` contains an `AS ENUM` clause (case-insensitive),
/// the marker for `CREATE TYPE ... AS ENUM (...)`.
///
/// `AS` and `ENUM` must be complete whitespace-delimited tokens, where the
/// `ENUM` token is either `ENUM` or `ENUM(` (the `(` opening the value list).
/// This rejects substring matches inside larger identifiers — e.g.
/// `has enumValues`, `AS enumValue`, `AS ENUMERATED` — which are prose, not DDL.
pub(super) fn declares_pg_enum(text: &str) -> bool {
    let upper = text.to_ascii_uppercase();
    let mut tokens = upper.split_whitespace().peekable();
    while let Some(tok) = tokens.next() {
        if tok == "AS"
            && tokens
                .peek()
                .is_some_and(|next| *next == "ENUM" || next.starts_with("ENUM("))
        {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod predicate_tests {
    use super::declares_pg_enum;

    #[test]
    fn fp_has_enum_values_not_flagged() {
        // Issue #3264: "has enumValues set as object" spells "as enum" across the
        // "has" + "enumValues" word boundary under a bare substring match.
        let text = "Should generate enum internal values resolvers \
                    when enum has enumValues set as object with explicit values";
        assert!(!declares_pg_enum(text));
        // Proof the old bare-`contains` logic flagged it.
        assert!(text.to_ascii_uppercase().contains("AS ENUM"));
    }

    #[test]
    fn flags_create_type_as_enum_with_space() {
        assert!(declares_pg_enum(
            "CREATE TYPE status AS ENUM ('active', 'inactive')"
        ));
    }

    #[test]
    fn flags_as_enum_no_space_before_paren() {
        assert!(declares_pg_enum("CREATE TYPE x AS ENUM('a','b')"));
    }

    #[test]
    fn allows_alias_named_enum_value() {
        assert!(!declares_pg_enum("SELECT col AS enumValue FROM t"));
    }

    #[test]
    fn allows_as_enumerated() {
        assert!(!declares_pg_enum("column AS ENUMERATED somewhere"));
    }
}
