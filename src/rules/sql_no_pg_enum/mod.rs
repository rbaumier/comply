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

/// True if `text` declares a concrete `CREATE TYPE ... AS ENUM ('a', 'b')`
/// statement: an `AS ENUM` clause (case-insensitive) followed by a value
/// list that holds at least one literal value.
///
/// `AS` and `ENUM` must be complete whitespace-delimited tokens, where the
/// `ENUM` token is either `ENUM` or `ENUM(` (the `(` opening the value list).
/// This rejects substring matches inside larger identifiers — e.g.
/// `has enumValues`, `AS enumValue`, `AS ENUMERATED` — which are prose, not DDL.
///
/// The value list must contain a literal Postgres value — a single-quoted
/// token (`'active'`). A concrete enum always single-quotes its values; a
/// DDL *builder* interpolates them (e.g. node-pg-migrate's
/// `` `CREATE TYPE ${name} AS ENUM (${values});` ``, whose value list is an
/// interpolation hole, or a string-concat fragment like `" AS ENUM ("` whose
/// parens are empty), so its literal text carries no quoted value and is not
/// a statement to flag — the user's `createType([...])` call site is.
pub(super) fn declares_pg_enum(text: &str) -> bool {
    let upper = text.to_ascii_uppercase();
    // Token-walk to find an `AS ENUM` / `AS ENUM(` boundary, tracking the byte
    // offset of the `ENUM` token so the value-list check starts right after it.
    let mut search_from = 0;
    while let Some(rel) = upper[search_from..].find("AS") {
        let as_start = search_from + rel;
        let as_end = as_start + 2;
        search_from = as_end;
        // `AS` must be a whole token (word boundaries on both sides).
        let before_ok = as_start == 0
            || !crate::rules::sql_helpers::is_ident_byte(upper.as_bytes()[as_start - 1]);
        if !before_ok {
            continue;
        }
        let rest = upper[as_end..].trim_start();
        let enum_at = as_end + (upper.len() - as_end - rest.len());
        // Next token must be `ENUM` (followed by a non-ident char) or `ENUM(`.
        let is_enum_token = rest.strip_prefix("ENUM").is_some_and(|after| {
            !after.starts_with(|c: char| c.is_ascii_alphanumeric() || c == '_')
        });
        if is_enum_token && enum_value_list_has_literal(&upper.as_bytes()[enum_at..]) {
            return true;
        }
    }
    false
}

/// True if the value list following the `ENUM` keyword holds at least one
/// literal Postgres value — a single-quoted token (`'active'`). Postgres enum
/// values are always single-quoted string literals; a DDL builder interpolates
/// them, so its literal text carries none.
fn enum_value_list_has_literal(after_enum: &[u8]) -> bool {
    after_enum.iter().any(|&b| b == b'\'')
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

    #[test]
    fn fp_interpolated_builder_value_list_not_flagged() {
        // Issue #5789: node-pg-migrate's createType builder template literal,
        // joined with the interpolation holes collapsed to whitespace, has an
        // empty value list — no literal value to flag. The user's createType
        // call site is the place to flag, not the builder.
        assert!(!declares_pg_enum("CREATE TYPE   AS ENUM (  );"));
    }

    #[test]
    fn fp_string_concat_fragment_not_flagged() {
        // A builder assembling the DDL via string concatenation emits the
        // `AS ENUM (` prefix as its own fragment with no value.
        assert!(!declares_pg_enum(" AS ENUM ("));
    }

    #[test]
    fn flags_literal_value_list_with_interpolated_type_name() {
        // Only the type name is interpolated; the values are literal — still a
        // concrete enum declaration.
        assert!(declares_pg_enum("CREATE TYPE   AS ENUM ('active', 'inactive')"));
    }
}
