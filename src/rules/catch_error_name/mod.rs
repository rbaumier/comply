//! catch-error-name — enforce the `error` name for `try { … } catch (…)`
//! parameters.
//!
//! ## Rationale
//!
//! Ported from `eslint-plugin-unicorn/catch-error-name`. Every catch in
//! the codebase uses the same binding name, so a grep for `error.` or
//! `error instanceof` finds every error-handling site without knowing
//! whether the local author typed `e`, `err`, `ex`, or `exception`.
//!
//! ## Language coverage
//!
//! **TypeScript / JavaScript / TSX**: handled by the `typescript`
//! backend. Flags `catch_clause` parameters that are simple identifiers
//! not matching the allowlist below.
//!
//! **Vue SFC (`<script>` and `<script setup>`)**: handled by the `vue`
//! backend. Extracts each `<script>` block via `vue_sfc::extract_scripts`,
//! re-parses its body with the TypeScript grammar, and runs the same
//! catch-parameter check. Diagnostic coordinates are translated back
//! to the Vue file.
//!
//! **Rust**: not covered. `Err(e)` is idiomatic in `match` / `if let`
//! and `.map_err(|e| …)`, and clippy explicitly does not flag it.
//! Forcing a unicorn/TS convention onto Rust pattern matching would
//! produce enormous noise without improving error handling.
//!
//! ## Allowed names
//!
//! - `_` — the parameter is unused. `catch (_) { return fallback; }`
//!   is a legitimate shape.
//! - `error` — the canonical name.
//! - `*error` / `*Error` — suffix match for disambiguated bindings
//!   like `networkError`, `parseError`, `httpError`, `readError`. The
//!   shape `innerError` is allowed for nested try/catch where the
//!   outer binding is already `error`.
//!
//! Everything else (`e`, `err`, `ex`, `exception`, `caughtException`,
//! `failure`, …) is flagged.

mod typescript;
mod vue;

#[cfg(test)]
mod shared_tests;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "catch-error-name",
    description: "The catch parameter should be named `error`.",
    remediation: "Rename the catch parameter to `error` (or a suffixed \
                  variant like `parseError` when disambiguating nested \
                  catches). Use `_` if the parameter is unused.",
    severity: Severity::Warning,
    doc_url: Some("https://github.com/sindresorhus/eslint-plugin-unicorn/blob/main/docs/rules/catch-error-name.md"),
    categories: &["unicorn"],
};

/// The canonical name the rule wants on the catch parameter.
pub(super) const EXPECTED: &str = "error";

/// Whether a catch-parameter identifier is acceptable.
///
/// Allowed shapes:
/// - `_` — deliberately-unused parameter.
/// - `error` — the canonical name.
/// - `*error` / `*Error` — suffix forms like `networkError`, `parseError`,
///   `innerError` (used to disambiguate nested catches that would
///   otherwise shadow the outer `error`).
pub(super) fn is_acceptable_name(name: &str) -> bool {
    name == "_" || name == EXPECTED || name.ends_with(EXPECTED) || name.ends_with("Error")
}

pub fn register() -> RuleDef {
    // Built manually (instead of `register_ts_family!`) so we can attach
    // a Vue backend on top of the TS/JS/TSX set. The Vue backend
    // re-parses `<script>` bodies with the TypeScript grammar and runs
    // the same catch-parameter check.
    RuleDef {
        meta: META,
        backends: vec![
            (Language::TypeScript, Backend::TreeSitter(Box::new(typescript::Check))),
            (Language::JavaScript, Backend::TreeSitter(Box::new(typescript::Check))),
            (Language::Tsx, Backend::TreeSitter(Box::new(typescript::Check))),
            (Language::Vue, Backend::TreeSitter(Box::new(vue::Check))),
        ],
    }
}

#[cfg(test)]
mod helper_tests {
    use super::*;

    #[test]
    fn accepts_canonical_error() {
        assert!(is_acceptable_name("error"));
    }

    #[test]
    fn accepts_underscore() {
        assert!(is_acceptable_name("_"));
    }

    #[test]
    fn accepts_suffixed_lower() {
        assert!(is_acceptable_name("networkerror"));
    }

    #[test]
    fn accepts_suffixed_upper() {
        assert!(is_acceptable_name("networkError"));
    }

    #[test]
    fn rejects_single_letter() {
        assert!(!is_acceptable_name("e"));
    }

    #[test]
    fn rejects_err() {
        assert!(!is_acceptable_name("err"));
    }

    #[test]
    fn rejects_ex() {
        assert!(!is_acceptable_name("ex"));
    }
}
