//! sql-nullable-requires-comment

mod text;
mod oxc_typescript;
mod vue;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "sql-nullable-requires-comment",
    description: "Nullable columns must have a `--` comment explaining why NULL is allowed.",
    remediation: "Add a `-- reason: <why this can be NULL>` comment on the preceding line.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["database", "sql"],

    skip_in_test_dir: true,
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
                Language::JavaScript,
                Backend::Oxc(Box::new(oxc_typescript::Check)),
            ),
            (
                Language::Tsx,
                Backend::Oxc(Box::new(oxc_typescript::Check)),
            ),
            (Language::Vue, Backend::TreeSitter(Box::new(vue::Check))),
            (Language::Sql, Backend::Text(Box::new(text::Check))),
        ],
    }
}

const SQL_TYPES: &[&str] = &[
    "INTEGER",
    "INT",
    "BIGINT",
    "SMALLINT",
    "TEXT",
    "VARCHAR",
    "CHAR",
    "BOOLEAN",
    "BOOL",
    "TIMESTAMP",
    "DATE",
    "DECIMAL",
    "NUMERIC",
    "FLOAT",
    "REAL",
    "DOUBLE",
    "UUID",
    "JSONB",
    "JSON",
    "BYTEA",
    "SERIAL",
    "BIGSERIAL",
];

/// True when `upper` (an already-uppercased SQL line) contains at least
/// one `SQL_TYPES` keyword as a whole word. A whole-word match requires
/// the characters immediately before and after the keyword to be word
/// boundaries (start/end of string, or a non `[A-Za-z0-9_]` character),
/// so `INTO` / `POINT` / `MINTED` no longer match the `INT` type while
/// `id INT,`, `amount INTEGER`, and `price INT(11)` still do.
pub(super) fn contains_sql_type_keyword(upper: &str) -> bool {
    let bytes = upper.as_bytes();
    let is_word_byte = |b: u8| b.is_ascii_alphanumeric() || b == b'_';
    SQL_TYPES.iter().any(|ty| {
        let ty_bytes = ty.as_bytes();
        upper.match_indices(ty).any(|(start, _)| {
            let before_ok = start == 0 || !is_word_byte(bytes[start - 1]);
            let end = start + ty_bytes.len();
            let after_ok = end == bytes.len() || !is_word_byte(bytes[end]);
            before_ok && after_ok
        })
    })
}

/// Returns the zero-based line offsets within `text` that declare a
/// nullable column without an inline `--` comment or a `--` comment on
/// the preceding line. Skips lines that already carry `NOT NULL`,
/// `PRIMARY KEY`, `CREATE`, `ALTER`, or are themselves comments.
///
/// Columns are read in `code`, comments in `text`.
/// `code` is `text` with the comments a caller blanks.
pub(super) fn nullable_lines_without_comment(text: &str, code: &str) -> Vec<usize> {
    let lines: Vec<&str> = text.lines().collect();
    let mut offsets = Vec::new();
    for (i, (line, code_line)) in lines.iter().zip(code.lines()).enumerate() {
        if !declares_nullable_column(code_line.to_ascii_uppercase().trim()) {
            continue;
        }
        let prev_is_comment = i > 0 && lines[i - 1].trim().starts_with("--");
        let has_inline_comment = line.contains("--");
        if !prev_is_comment && !has_inline_comment {
            offsets.push(i);
        }
    }
    offsets
}

/// True when `line`, uppercased, declares a column that accepts NULL.
fn declares_nullable_column(line: &str) -> bool {
    let refuses_null = line.contains("NOT NULL") || line.contains("PRIMARY KEY");
    let is_head = line.starts_with("CREATE") || line.starts_with("ALTER") || line.starts_with("--");
    contains_sql_type_keyword(line) && !refuses_null && !is_head
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keyword_match_respects_word_boundaries() {
        // `INTO` (as in INSERT INTO) must not match the `INT` type.
        assert!(!contains_sql_type_keyword("INSERT INTO TEAMS (ID, NAME)"));
        // `POINT` / `GEOMETRY` must not be classified as `INT`.
        assert!(!contains_sql_type_keyword("POINT GEOMETRY"));
        // Genuine type tokens still match.
        assert!(contains_sql_type_keyword("AGE INT,"));
        assert!(contains_sql_type_keyword("SCORE INTEGER"));
        assert!(contains_sql_type_keyword("PRICE INT(11)"));
        assert!(contains_sql_type_keyword("AVATAR_URL TEXT,"));
    }

    #[test]
    fn insert_statements_are_not_nullable_columns_issue_1340() {
        // Regression for #1340: the INSERT lines (which contain `INTO`) are
        // DML, not column definitions, and the real columns are NOT NULL —
        // so the block must yield zero nullable-column offsets.
        let sql = "\
create table teams (
  id int primary key not null,
  name text not null unique
);
insert into teams (id, name) values (1, 'a');
insert into teams (id, name) values (2, 'b');";
        assert!(nullable_lines_without_comment(sql, sql).is_empty());
    }

    #[test]
    fn genuine_nullable_column_still_flagged() {
        // Negative-space guard: a real nullable column with no NOT NULL and
        // no `--` comment must still be reported.
        assert_eq!(nullable_lines_without_comment("  age INT,", "  age INT,").len(), 1);
        assert_eq!(nullable_lines_without_comment("  score INTEGER", "  score INTEGER").len(), 1);
    }
}
