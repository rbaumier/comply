//! sql-nullable-requires-comment

mod rust;
mod text;
mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "sql-nullable-requires-comment",
    description: "Nullable columns must have a `--` comment explaining why NULL is allowed.",
    remediation: "Add a `-- reason: <why this can be NULL>` comment on the preceding line.",
    severity: Severity::Warning,
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
            (Language::Rust, Backend::TreeSitter(Box::new(rust::Check))),
            (Language::Vue, Backend::Text(Box::new(text::Check))),
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

/// Returns the zero-based line offsets within `text` that declare a
/// nullable column without an inline `--` comment or a `--` comment on
/// the preceding line. Skips lines that already carry `NOT NULL`,
/// `PRIMARY KEY`, `CREATE`, `ALTER`, or are themselves comments.
pub(super) fn nullable_lines_without_comment(text: &str) -> Vec<usize> {
    let lines: Vec<&str> = text.lines().collect();
    let mut offsets = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        let upper = line.to_ascii_uppercase();
        let t = upper.trim();
        if !SQL_TYPES.iter().any(|ty| t.contains(ty)) {
            continue;
        }
        if t.contains("NOT NULL") || t.contains("PRIMARY KEY") {
            continue;
        }
        if t.starts_with("CREATE") || t.starts_with("ALTER") || t.starts_with("--") {
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
