//! sql-prefer-exists-over-in

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
    id: "sql-prefer-exists-over-in",
    description: "`WHERE x IN (SELECT ...)` — prefer `EXISTS` which exits on first match.",
    remediation: "Replace `WHERE col IN (SELECT ...)` with `WHERE EXISTS (SELECT 1 FROM ... WHERE ...)`. EXISTS short-circuits on the first match; IN must materialize the entire subquery.",
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
            (Language::Vue, Backend::Text(Box::new(text::Check))),
            (Language::Sql, Backend::Text(Box::new(text::Check))),
        ],
    }
}

/// True if `text` contains `IN (SELECT ...)` (case-insensitive),
/// indicating an `IN`-with-subquery pattern that should be `EXISTS`.
pub(super) fn contains_in_subquery(text: &str) -> bool {
    let upper = text.to_ascii_uppercase();
    upper.contains("IN (SELECT") || upper.contains("IN(SELECT")
}

const TEST_MARKERS: &[&str] = &[".test.", ".spec.", "__tests__", "_test.", ".e2e."];

/// True if `path` looks like a test file — performance warnings are irrelevant
/// in test teardown code.
pub(super) fn is_test_file(path: &std::path::Path) -> bool {
    let s = path.to_string_lossy();
    TEST_MARKERS.iter().any(|m| s.contains(m))
}
