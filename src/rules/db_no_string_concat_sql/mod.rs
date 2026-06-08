//! db-no-string-concat-sql — SQL injection: variable concatenated into SQL.
//!
//! ## Why this rule was rewritten
//!
//! The previous implementation scanned the entire AST node text
//! (whole `macro_invocation` for Rust, whole `binary_expression` for
//! TS) for SQL keywords as substrings. That had two failure modes:
//!
//! - **Substring match without word boundaries**: `String::from_utf8_lossy`
//!   uppercased contains `FROM_UTF8` which contains `FROM`, so any
//!   `format!` call passing `String::from_utf8_lossy(...)` got
//!   flagged as SQL injection — the user's exact FP on
//!   `src/oxlint/mod.rs:106`.
//! - **Wrong scope**: the keyword scan looked at the full macro
//!   invocation including the *arguments*, not just the format
//!   string. Identifiers in the arg list could contain SQL keyword
//!   substrings and false-positive.
//!
//! ## How the new rule works
//!
//! Per backend, the detection now isolates the *static SQL surface*:
//!
//! - **Rust** (`format!` / `write!` / `writeln!` / `print!` / `println!`):
//!   walk the macro invocation's `token_tree`, find the first
//!   string literal (raw or normal), and check `is_sql_string` on
//!   that text alone. The args are ignored.
//! - **TS** (binary `+` concat): for each side of the
//!   `binary_expression` that is a `string` or `template_string`,
//!   extract the literal text and run `is_sql_string` on it. If
//!   any side is a real SQL string AND the other side is a
//!   non-literal expression, flag.
//! - **Vue**: extract `<script>` blocks via `vue_sfc::extract_scripts`,
//!   re-parse with the TS grammar, run the same TS logic with
//!   coordinate translation.
//!
//! The whole-word matching in `sql_helpers::is_sql_string` (DML +
//! WHERE/FROM) eliminates `from_utf8` and similar identifier
//! substring false positives.

mod oxc_typescript;
mod rust;
mod vue;

#[cfg(test)]
mod shared_tests;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "db-no-string-concat-sql",
    description: "String concatenation with SQL keywords is a SQL injection vector.",
    remediation: "Use parameterized queries (`$1`, `?`, or ORM methods) instead \
                  of string concatenation. Never interpolate user input into SQL \
                  strings.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["database"],

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
            (Language::Vue, Backend::TreeSitter(Box::new(vue::Check))),
        ],
    }
}
