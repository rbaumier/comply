//! sql-no-truncate-in-app

mod rust;
mod sql;
mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "sql-no-truncate-in-app",
    description: "`TRUNCATE` bypasses triggers, FK checks, and row-level audit.",
    remediation: "Use `DELETE FROM table` so triggers, FK cascades and audit logs fire. `TRUNCATE` belongs to ops-only maintenance scripts, not application queries.",
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
                Language::JavaScript,
                Backend::Oxc(Box::new(oxc_typescript::Check)),
            ),
            (
                Language::Tsx,
                Backend::Oxc(Box::new(oxc_typescript::Check)),
            ),
            (Language::Rust, Backend::TreeSitter(Box::new(rust::Check))),
            (Language::Sql, Backend::Text(Box::new(sql::Check))),
        ],
    }
}

/// True if the string contains the SQL `TRUNCATE` statement (whole word).
pub(super) fn sql_uses_truncate(sql: &str) -> bool {
    let lower = sql.to_ascii_lowercase();
    crate::rules::sql_helpers::contains_word(&lower, "truncate")
}

/// True if the string looks like an actual SQL `TRUNCATE` statement.
///
/// Tailwind CSS exposes a `truncate` utility class that frequently appears
/// in JSX `className` strings (`<span className="truncate flex">`,
/// `[&>span:last-child]:truncate`). Matching the bare word `TRUNCATE` —
/// or even `TRUNCATE ` followed by any whitespace — false-positives on
/// every component using that utility.
///
/// Real SQL `TRUNCATE` always takes one of these forms:
///   - `TRUNCATE TABLE <name>` (canonical)
///   - `TRUNCATE [ONLY] <name> [, <name>]* [ RESTART IDENTITY | CONTINUE
///     IDENTITY | CASCADE | RESTRICT ]`
///
/// We accept a string when:
///   1. It contains `TRUNCATE TABLE` (definitive — Tailwind never produces
///      this), OR
///   2. It contains `TRUNCATE` followed by an identifier AND another SQL
///      signal (`CASCADE`, `RESTART IDENTITY`, `CONTINUE IDENTITY`,
///      `RESTRICT`, `ONLY`, or a trailing `;`).
pub(super) fn looks_like_sql_truncate(text: &str) -> bool {
    let upper = text.to_ascii_uppercase();
    if upper.contains("TRUNCATE TABLE") {
        return true;
    }
    if !truncate_followed_by_ident(&upper) {
        return false;
    }
    // Trim surrounding string-literal delimiters (`"` / `'` / `` ` ``)
    // and trailing whitespace so a SQL string with a closing `;` still
    // matches when the AST node text includes the quotes.
    let trimmed = text
        .trim_end_matches(|c: char| c.is_ascii_whitespace() || c == '"' || c == '\'' || c == '`');
    trimmed.ends_with(';')
        || upper.contains("CASCADE")
        || upper.contains("RESTRICT")
        || upper.contains("RESTART IDENTITY")
        || upper.contains("CONTINUE IDENTITY")
        || upper.contains("TRUNCATE ONLY ")
}

/// True if `upper` (already uppercased) contains `TRUNCATE` as a whole
/// word followed (after whitespace) by an identifier-like token —
/// i.e. starts with an ASCII letter or underscore.
fn truncate_followed_by_ident(upper: &str) -> bool {
    let bytes = upper.as_bytes();
    let needle = b"TRUNCATE";
    let mut i = 0;
    while i + needle.len() <= bytes.len() {
        if &bytes[i..i + needle.len()] == needle {
            let before_ok = i == 0 || !is_ident_byte(bytes[i - 1]);
            let after_idx = i + needle.len();
            let after_ok = after_idx >= bytes.len() || !is_ident_byte(bytes[after_idx]);
            if before_ok && after_ok {
                let mut j = after_idx;
                while j < bytes.len() && bytes[j].is_ascii_whitespace() {
                    j += 1;
                }
                if j < bytes.len() && (bytes[j].is_ascii_alphabetic() || bytes[j] == b'_') {
                    return true;
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
    use super::looks_like_sql_truncate;

    #[test]
    fn matches_truncate_table() {
        assert!(looks_like_sql_truncate("TRUNCATE TABLE users"));
        assert!(looks_like_sql_truncate("truncate table users"));
    }

    #[test]
    fn matches_truncate_with_cascade() {
        assert!(looks_like_sql_truncate("TRUNCATE users CASCADE"));
    }

    #[test]
    fn matches_truncate_with_semicolon() {
        assert!(looks_like_sql_truncate("TRUNCATE users;"));
    }

    #[test]
    fn rejects_tailwind_truncate_class() {
        assert!(!looks_like_sql_truncate("truncate"));
        assert!(!looks_like_sql_truncate("truncate flex items-center"));
        assert!(!looks_like_sql_truncate("flex items-center truncate"));
        assert!(!looks_like_sql_truncate("[&>span:last-child]:truncate"));
        assert!(!looks_like_sql_truncate(
            "text-sm truncate text-gray-500 max-w-xs"
        ));
    }

    #[test]
    fn rejects_prose_with_truncate() {
        assert!(!looks_like_sql_truncate("we truncate the value here"));
    }
}
